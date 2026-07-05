use std::collections::HashMap;

use crate::geo::Systems;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DetectedSystem {
    pub id: i64,
    pub name: String,
    pub security: f64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DetectedShip {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Movement {
    pub from: String,
    pub jumps: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Probes {
    Core,
    Combat,
    Any,
}

impl Probes {
    pub fn label(self) -> &'static str {
        match self {
            Probes::Core => "Core Probes",
            Probes::Combat => "Combat Probes",
            Probes::Any => "Probes",
        }
    }
}

impl std::fmt::Display for Probes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AnomKind {
    Anomaly,
    Signature,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct IntelReport {
    pub id: u64,
    pub received: i64,
    pub channel: String,
    pub reporter: String,
    pub text: String,
    pub systems: Vec<DetectedSystem>,
    pub ships: Vec<DetectedShip>,
    pub classes: Vec<String>,
    pub pilots: Vec<String>,
    pub count: Option<u32>,
    /// Count components, kept so `count` can be re-derived when the pilot list changes during
    /// resolution (a discarded candidate must stop inflating the hostile count). `count_extra` is
    /// an explicitly stated total (x5, a ship count), `count_plus` a `+N` addend, `count_ships`
    /// the resolved "Name N" ship counts, `solo` the solo keyword. See `derive_count`.
    #[serde(default)]
    pub count_extra: Option<u32>,
    #[serde(default)]
    pub count_plus: u32,
    #[serde(default)]
    pub count_ships: u32,
    #[serde(default)]
    pub solo: bool,
    pub name_number_skips: Vec<(String, u32)>,
    pub isk: Option<u64>,
    pub structures: Vec<(String, Option<String>)>,
    pub celestials: Vec<String>,
    pub probes: Option<Probes>,
    pub clear: bool,
    pub status: bool,
    pub no_visual: bool,
    pub spike: bool,
    pub camp: bool,
    pub help: bool,
    pub bubble: bool,
    /// The reporter noted an Interdiction Nullifier (the ship ignores bubbles).
    #[serde(default)]
    pub nullified: bool,
    pub killmail: bool,
    #[serde(default)]
    pub near_celestial: Option<(String, f64)>,
    pub cyno: bool,
    pub dropper: bool,
    pub cap_tackled: bool,
    pub tackled: bool,
    pub tackled_targets: Vec<String>,
    pub wormhole: bool,
    pub wh_type: Option<String>,
    pub wh_dest: Option<crate::wormholes::DestClass>,
    #[serde(default)]
    pub wh_size: Option<crate::wormholes::ShipSize>,
    pub wh_eol: bool,
    pub wh_drifter: bool,
    pub wh_sig: Option<String>,
    pub ess: bool,
    pub ess_time: Option<String>,
    pub skyhook: bool,
    pub filament: bool,
    pub diamond_rats: bool,
    pub anom_sigs: Vec<(AnomKind, String)>,
    pub gates: Vec<String>,
    pub alliances: Vec<(String, i64)>,
    pub movement: Option<Movement>,
    pub links: Vec<IntelLink>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LinkKind {
    Killmail,
    BattleReport,
    Dscan,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IntelLink {
    pub kind: LinkKind,
    pub url: String,
    pub kill_id: Option<i64>,
}

fn strip_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for tok in text.split_inclusive(char::is_whitespace) {
        let word = tok.trim_end_matches(char::is_whitespace);
        let bare = word.trim_start_matches(|c: char| "<>()[]\"'".contains(c));
        if bare.starts_with("http://") || bare.starts_with("https://") {
            out.extend(word.chars().map(|_| ' '));
            out.push_str(&tok[word.len()..]);
        } else {
            out.push_str(tok);
        }
    }
    out
}

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
        } else if lower.contains("br.evetools.org")
            || lower.contains("zkillboard.com/related/")
            || lower.contains("eve-spai.com/br/")
        {
            IntelLink { kind: LinkKind::BattleReport, url: url.to_owned(), kill_id: None }
        } else if lower.contains("dscan.me")
            || lower.contains("dscan.org")
            || lower.contains("dscan.info")
            || lower.contains("adashboard.info")
        {
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
    pub fn primary_system(&self) -> Option<&DetectedSystem> {
        self.systems.first()
    }
}

#[derive(Default)]
pub struct IntelState {
    pub reports: Vec<IntelReport>,
    cleared: HashMap<String, i64>,
    orphans: Vec<IntelReport>,
}

static NEXT_REPORT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl IntelState {
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

    pub fn try_amend(&mut self, new: &IntelReport, grace: i64, systems: &Systems) -> bool {
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
            || new.filament
            || new.nullified
            || new.cap_tackled;
        if !adds {
            return false;
        }
        let new_sys = new.primary_system().map(|s| s.id);
        let new_pilots: std::collections::HashSet<String> =
            new.pilots.iter().map(|p| p.to_lowercase()).collect();
        let name_words = |pilots: &[String]| -> std::collections::HashSet<String> {
            pilots
                .iter()
                .flat_map(|p| p.split_whitespace())
                .filter(|w| !is_system_token(w, systems))
                .map(|w| w.to_lowercase())
                .collect()
        };
        let new_words = name_words(&new.pilots);
        for prev in self.reports.iter_mut().rev() {
            if prev.clear {
                continue;
            }
            let same_reporter = prev.reporter == new.reporter;
            let shares_pilot = (!new_pilots.is_empty()
                && prev.pilots.iter().any(|p| new_pilots.contains(&p.to_lowercase())))
                || (!new_words.is_empty()
                    && name_words(&prev.pilots).intersection(&new_words).next().is_some());
            if !same_reporter && !shares_pilot {
                continue;
            }
            if new.received < prev.received || new.received - prev.received > grace {
                continue;
            }
            let prev_sys = prev.primary_system().map(|s| s.id);
            if new_sys.is_some() && prev_sys.is_some() && new_sys != prev_sys {
                continue;
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
            let merge_src = format!("{} {}", prev.text, new.text);
            drop_subphrase_pilots(&mut prev.pilots, &std::collections::HashSet::new(), &merge_src);
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
            prev.count_extra = match (prev.count_extra, new.count_extra) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            prev.count_plus = prev.count_plus.max(new.count_plus);
            prev.count_ships = prev.count_ships.max(new.count_ships);
            prev.solo = prev.solo || new.solo;
            prev.count = derive_count(
                prev.count_extra,
                prev.count_plus,
                prev.count_ships,
                prev.pilots.len() as u32,
                prev.solo,
            );
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
            prev.nullified |= new.nullified;
            prev.cyno |= new.cyno;
            prev.filament |= new.filament;
            prev.diamond_rats |= new.diamond_rats;
            for asig in &new.anom_sigs {
                if !prev.anom_sigs.iter().any(|(k, c)| *k == asig.0 && c.eq_ignore_ascii_case(&asig.1)) {
                    prev.anom_sigs.push(asig.clone());
                }
            }
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
            prev.wh_size = new.wh_size.or(prev.wh_size);
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
            prev.received = new.received;
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

    pub fn stash_orphan(&mut self, report: IntelReport, grace: i64, now: i64) {
        self.orphans.retain(|o| now - o.received <= grace);
        self.orphans.push(report);
    }

    pub fn reverse_amend(&mut self, new: &mut IntelReport, grace: i64) -> usize {
        if new.clear || new.systems.is_empty() {
            self.orphans.retain(|o| new.received < o.received || new.received - o.received <= grace);
            return 0;
        }
        let mut merged = 0usize;
        let mut kept: Vec<IntelReport> = Vec::with_capacity(self.orphans.len());
        for o in std::mem::take(&mut self.orphans) {
            let stale = new.received < o.received || new.received - o.received > grace;
            if o.reporter == new.reporter && o.channel == new.channel && !stale {
                merge_report_into(new, &o);
                merged += 1;
            } else if !stale {
                kept.push(o);
            }
        }
        self.orphans = kept;
        merged
    }

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

fn merge_report_into(dst: &mut IntelReport, src: &IntelReport) {
    for sh in &src.ships {
        if !dst.ships.iter().any(|s| s.id == sh.id) {
            dst.ships.push(sh.clone());
        }
    }
    for c in &src.classes {
        if !dst.classes.iter().any(|x| x.eq_ignore_ascii_case(c)) {
            dst.classes.push(c.clone());
        }
    }
    for p in &src.pilots {
        if !dst.pilots.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            dst.pilots.push(p.clone());
        }
    }
    let merge_src = format!("{} {}", src.text, dst.text);
    drop_subphrase_pilots(&mut dst.pilots, &std::collections::HashSet::new(), &merge_src);
    for a in &src.alliances {
        if !dst.alliances.iter().any(|(n, _)| n.eq_ignore_ascii_case(&a.0)) {
            dst.alliances.push(a.clone());
        }
    }
    for g in &src.gates {
        if !dst.gates.iter().any(|x| x.eq_ignore_ascii_case(g)) {
            dst.gates.push(g.clone());
        }
    }
    for c in &src.celestials {
        if !dst.celestials.iter().any(|x| x.eq_ignore_ascii_case(c)) {
            dst.celestials.push(c.clone());
        }
    }
    for sk in &src.name_number_skips {
        if !dst.name_number_skips.iter().any(|(c, _)| c.eq_ignore_ascii_case(&sk.0)) {
            dst.name_number_skips.push(sk.clone());
        }
    }
    for tt in &src.tackled_targets {
        if !dst.tackled_targets.iter().any(|x| x.eq_ignore_ascii_case(tt)) {
            dst.tackled_targets.push(tt.clone());
        }
    }
    for asig in &src.anom_sigs {
        if !dst.anom_sigs.iter().any(|(k, c)| *k == asig.0 && c.eq_ignore_ascii_case(&asig.1)) {
            dst.anom_sigs.push(asig.clone());
        }
    }
    for (n, d) in &src.structures {
        match dst.structures.iter_mut().find(|(pn, _)| pn == n) {
            Some(e) => {
                if e.1.is_none() {
                    e.1 = d.clone();
                }
            }
            None => dst.structures.push((n.clone(), d.clone())),
        }
    }
    for l in &src.links {
        if !dst.links.iter().any(|p| p.url == l.url) {
            dst.links.push(l.clone());
        }
    }
    dst.count_extra = match (dst.count_extra, src.count_extra) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    dst.count_plus = dst.count_plus.max(src.count_plus);
    dst.count_ships = dst.count_ships.max(src.count_ships);
    dst.solo = dst.solo || src.solo;
    dst.count = derive_count(
        dst.count_extra,
        dst.count_plus,
        dst.count_ships,
        dst.pilots.len() as u32,
        dst.solo,
    );
    dst.isk = match (dst.isk, src.isk) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    dst.probes = dst.probes.or(src.probes);
    dst.status |= src.status;
    dst.no_visual |= src.no_visual;
    dst.spike |= src.spike;
    dst.camp |= src.camp;
    dst.help |= src.help;
    dst.bubble |= src.bubble;
    dst.nullified |= src.nullified;
    dst.cyno |= src.cyno;
    dst.dropper |= src.dropper;
    dst.cap_tackled |= src.cap_tackled;
    dst.tackled |= src.tackled;
    dst.killmail |= src.killmail;
    dst.filament |= src.filament;
    dst.diamond_rats |= src.diamond_rats;
    dst.wormhole |= src.wormhole;
    dst.wh_type = dst.wh_type.clone().or_else(|| src.wh_type.clone());
    dst.wh_dest = dst.wh_dest.or(src.wh_dest);
    dst.wh_size = dst.wh_size.or(src.wh_size);
    dst.wh_eol |= src.wh_eol;
    dst.wh_drifter |= src.wh_drifter;
    dst.wh_sig = dst.wh_sig.clone().or_else(|| src.wh_sig.clone());
    dst.ess |= src.ess;
    dst.ess_time = dst.ess_time.clone().or_else(|| src.ess_time.clone());
    dst.skyhook |= src.skyhook;
    dst.text = format!("{}  ·  {}", src.text, dst.text);
}

const CLEAR_WORDS: &[&str] = &["clear", "clr", "cleared", "clr+", "safe"];

const KEYWORD_NAME_PILOTS: &[&str] = &["Clean cyno toon", "RSS Scanner Probe", "clear rain"];

const PILOT_STOP: &[&str] = &[
    "gate", "gates", "stargate", "stargates", "camp", "camper", "campers", "gatecamp", "gatecamps", "clear", "clr", "cleared", "spike", "bubble", "drag", "dragbubble", "cyno", "local", "dock", "docked",
    "solo",
    "type", "types", "shiptype", "shiptypes",
    "station", "kill", "killmail", "dead", "ded", "pod", "no", "visual", "nv", "nvm", "ess", "skyhook", "hostile",
    "filament", "filaments", "needlejack", "needlejacks", "trace", "traces",
    "hostiles", "neut", "neutral", "neuts", "red", "reds", "blue", "blues", "gang", "fleet",
    "bridge", "jump", "jumping", "warp", "warping", "the", "incoming", "inc", "coming", "gcc",
    "afk", "warpin", "system", "and", "for", "status", "stat", "eyes", "any", "report", "intel", "went", "going",
    "help", "sos", "backup", "need",
    "guys", "in", "space",
    "just", "is", "are", "was", "were", "be", "been", "has", "have", "had", "not", "but",
    "now", "still", "back", "with", "this", "that", "they", "them", "their", "here", "there", "to",
    "crit", "wrong", "channel", "see", "nothing", "else", "safe",
    "whoever", "whatever", "whenever", "wherever", "however", "someone", "somebody", "anybody",
    "everyone", "everybody", "nobody",
    "about", "after", "because", "call", "came", "can", "come", "could", "day", "did", "die",
    "even", "feel", "find", "first", "form", "get", "give", "good", "her", "his", "its", "keep",
    "know", "leave", "let", "like", "look", "lose", "love", "make", "mean", "most", "new", "our",
    "pay", "people", "put", "read", "run", "said", "saw", "send", "set", "she", "show", "stand",
    "start", "stay", "take", "talk", "tell", "than", "then", "these", "time", "took", "try", "two",
    "understand", "use", "want", "watch", "will", "win", "work", "worm",
    "from", "got", "off", "out", "near", "into", "onto", "over", "your", "youre", "again",
    "rest", "stop",
    "hacking", "hack", "hacked", "ratting", "ratted", "missing", "guess",
    "think", "thought", "believe", "maybe", "probably", "prob", "probs",
    "clean", "reported", "yet",
    "50mn", "fit",
    "on", "grid", "ongrid", "offgrid", "few", "possible", "atm", "many", "outside", "entrance",
    "linked", "side",
    "theft", "stealing", "stole", "bash", "bashing", "reinforced", "reinforce", "rf",
    "drop", "dropper", "droppers", "hotdrop", "hotdrops", "hotdropper", "hotdroppers",
    "hotdropping", "blops", "blackops", "blackop",
    "fight", "fights", "fighting", "engaged", "engage", "engaging",
    "etc",
    "more",
    "scan", "scans", "dscan", "scanning",
    "drifter", "drifters",
    "him", "other", "only", "unless", "end", "also", "confirm", "confirmed", "clearing",
    "enemies", "enemy", "mostly", "around", "an", "roaming", "somewhere", "support",
    "unsure", "which", "too", "kitchen", "sink", "catch", "all",
    "what", "where", "when", "who", "why", "how", "well", "anyway", "huh", "hmm", "hmmm",
    "wait", "sure", "dunno", "yes", "yeah", "yep", "yup", "nope", "nah", "ok", "okay", "kk",
    "sry", "sorry", "ty", "tyvm", "thx", "thanks", "thanx", "np", "yw", "cheers", "lol",
    "lmao", "rofl", "omg", "omw", "wtf", "wth", "ffs", "gg", "wp", "ez", "gj", "gz", "grats",
    "imo", "tbh", "idk", "ikr", "btw", "fyi", "pls", "plz", "plox", "brb", "gtg", "glhf",
    "gl", "hf", "cya", "ttyl", "sup", "yo", "o7", "07", "rip",
    "im", "i'm", "youre", "you're", "theyre", "they're", "we're",
    "its", "it's", "dont", "don't", "cant", "can't", "wont", "won't",
    "thats", "that's", "whats", "what's", "lets", "let's", "gonna", "wanna",
];

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

/// A bare hull tier (frigate..battleship) is just a size, not worth a badge. Only specialised
/// (T2/T3) and capital classes matter.
fn is_generic_hull_class(class: &str) -> bool {
    matches!(class, "Frigate" | "Destroyer" | "Cruiser" | "Battlecruiser" | "Battleship")
}

fn detect_classes(
    lower_tokens: &[String],
    pilot_tokens: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in lower_tokens {
        // A class word that belongs to a pilot's name ("... Destroyer") is not a class report.
        if pilot_tokens.contains(t) {
            continue;
        }
        if let Some((_, class)) = SHIP_CLASSES.iter().find(|(k, _)| *k == t.as_str()) {
            if is_generic_hull_class(class) {
                continue;
            }
            if !out.iter().any(|c| c == class) {
                out.push((*class).to_owned());
            }
        }
    }
    out
}

pub fn is_pilot_stopword(w: &str) -> bool {
    let lw = w.to_lowercase();
    if lw.split_whitespace().nth(1).is_some() {
        return lw.split_whitespace().all(is_pilot_stopword);
    }
    PILOT_STOP.contains(&lw.as_str())
        || SHIP_CLASSES.iter().any(|(k, _)| *k == lw.as_str())
        || matches!(
            lw.as_str(),
            "ship" | "ships" | "shuttle" | "shuttles" | "navy" | "issue" | "loc"
                | "location" | "likely" | "probably" | "maybe" | "checking" | "left" | "went" | "min" | "mins" | "minute" | "minutes"
                | "heading" | "towards" | "toward" | "through" | "inbound" | "enroute"
                | "between"
                | "total" | "anchored" | "anchor" | "anchoring"
                | "bank" | "reserve" | "main"
                | "small" | "large" | "big" | "huge" | "full"
                | "sig" | "sigs" | "anyone" | "currently"
                | "anom" | "anomaly" | "anomalies" | "signature" | "signatures"
                | "rat" | "rats" | "diamond" | "dia"
                | "probe" | "probes" | "prob" | "probs" | "combat" | "core" | "scanner" | "sisters"
                | "ivy"
                | "jumped" | "jumping" | "warped" | "landed" | "burning" | "aligning"
                | "incoming" | "inc" | "primary" | "killed" | "podded"
                | "wormhole" | "wormholes" | "hole" | "holes" | "wh"
                | "bubbled" | "bubbles" | "bubbling" | "cloak" | "cloaked" | "cloaky"
                | "cloaks" | "cloaking" | "cloacked" | "cloack" | "cloacking"
                | "decloak" | "decloaked" | "camped" | "camping"
                | "ansi" | "ansiblex" | "jumpbridge" | "bridge" | "jump" | "jumps"
                | "pls" | "plz"
                | "dic" | "dics" | "dictor" | "dictors" | "interdictor" | "interdictors"
                | "hic" | "hics" | "hictor" | "hictors" | "recon" | "recons" | "bomber"
                | "bombers" | "logi" | "logis" | "ceptor" | "ceptors" | "hac" | "hacs"
                | "marauder" | "marauders" | "blops"
                | "tackled" | "tackle" | "tackling" | "takled" | "pointed" | "point"
                | "scrammed" | "scram" | "scrambled" | "webbed"
                | "nullified" | "nullifier" | "nullifiers" | "nullification" | "nully" | "nullie" | "nullies"
        )
}

pub fn is_lowercaseish(w: &str) -> bool {
    w == "I" || !w.chars().any(|c| c.is_ascii_uppercase())
}

fn extract_quoted(text: &str) -> Vec<String> {
    let is_quote = |c: char| c == '"' || c == '\'' || c == '`';
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if is_quote(chars[i]) && (i == 0 || chars[i - 1].is_whitespace()) {
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

fn name_part(t: &str) -> bool {
    t.len() >= 2
        && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
        && t.chars().any(|c| c.is_ascii_alphabetic())
        && (!t.contains('-') || t.chars().any(|c| c.is_ascii_lowercase()))
}

fn looks_like_system_code(t: &str) -> bool {
    if t.len() < 2 || !t.contains('-') {
        return false;
    }
    if t.starts_with('-') {
        return false;
    }
    if t.len() > 6 {
        return false;
    }
    if !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || !t.chars().any(|c| c.is_ascii_alphanumeric())
    {
        return false;
    }
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let longest_segment = t.split('-').map(|s| s.len()).max().unwrap_or(0);
    has_digit || longest_segment <= 3
}

fn is_short_code_token(t: &str) -> bool {
    let n = t.chars().count();
    (2..=5).contains(&n)
        && t.chars().all(|c| c.is_ascii_alphanumeric())
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

fn is_code_lookalike_name(t: &str, systems: &Systems) -> bool {
    looks_like_system_code(t)
        && t.chars().any(|c| c.is_ascii_lowercase())
        && resolve(systems, t).is_none()
        && systems.lookup_prefix(t).is_none()
}

fn looks_like_anom_code(t: &str) -> bool {
    let n = t.chars().count();
    if !(3..=8).contains(&n) || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }
    let leading_letters = t.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    leading_letters >= 3
        && t.chars().skip(leading_letters).all(|c| c == '-' || c.is_ascii_digit())
}

fn detect_diamond_rats(tokens: &[&str]) -> (bool, Vec<String>) {
    let mut hit = false;
    let mut consumed: Vec<String> = Vec::new();
    for w in tokens.windows(2) {
        let (a, b) = (w[0].to_lowercase(), w[1].to_lowercase());
        if matches!(a.as_str(), "diamond" | "dia") && matches!(b.as_str(), "rat" | "rats") {
            hit = true;
            consumed.push(a);
            consumed.push(b);
        }
    }
    (hit, consumed)
}

fn detect_anom_sigs(tokens: &[&str], systems: &Systems) -> (Vec<(AnomKind, String)>, Vec<String>) {
    let kind_of = |w: &str| match w {
        "anom" | "anomaly" | "anomalies" => Some(AnomKind::Anomaly),
        "sig" | "sigs" | "signature" | "signatures" => Some(AnomKind::Signature),
        _ => None,
    };
    let mut out: Vec<(AnomKind, String)> = Vec::new();
    let mut consumed: Vec<String> = Vec::new();
    for i in 0..tokens.len() {
        let Some(kind) = kind_of(&tokens[i].to_lowercase()) else { continue };
        consumed.push(tokens[i].to_lowercase());
        let mut code = String::new();
        for j in [i.checked_sub(1), Some(i + 1)].into_iter().flatten() {
            let Some(tok) = tokens.get(j) else { continue };
            if !looks_like_anom_code(tok) {
                continue;
            }
            let lc = tok.to_lowercase();
            if !tok.chars().any(|c| c.is_ascii_digit())
                && (crate::dict::is_word(&lc) || is_pilot_stopword(&lc))
            {
                continue;
            }
            if is_system_token(tok, systems)
                || resolve(systems, tok).is_some()
                || systems.lookup_prefix(&lc).is_some()
            {
                continue;
            }
            code = tok.to_uppercase();
            consumed.push(lc);
            break;
        }
        if !out.iter().any(|(k, c)| *k == kind && c.eq_ignore_ascii_case(&code)) {
            out.push((kind, code));
        }
    }
    let coded: Vec<AnomKind> = out.iter().filter(|(_, c)| !c.is_empty()).map(|(k, _)| *k).collect();
    out.retain(|(k, c)| !c.is_empty() || !coded.contains(k));
    (out, consumed)
}

fn is_time_token(t: &str) -> bool {
    let lower = t.to_lowercase();
    let Some(de) = lower.find(|c: char| !c.is_ascii_digit()) else {
        return false;
    };
    if de == 0 {
        return false;
    }
    matches!(
        &lower[de..],
        "min" | "mins" | "minute" | "minutes" | "m" | "s" | "sec" | "secs"
            | "second" | "seconds" | "h" | "hr" | "hrs" | "hour" | "hours" | "d"
    )
}

fn is_amount_token(t: &str) -> bool {
    let lower = t.to_lowercase();
    let de = match lower.find(|c: char| !c.is_ascii_digit() && c != '.') {
        Some(0) | None => return false,
        Some(de) => de,
    };
    matches!(
        &lower[de..],
        "k" | "kk" | "m" | "mil" | "mill" | "million" | "millions" | "mio" | "mio."
            | "b" | "bil" | "bill" | "billion" | "billions" | "isk"
    )
}

fn is_distinctive_name(t: &str) -> bool {
    name_part(t)
        && !looks_like_system_code(t)
        && (t.contains('-')
            || t.contains('\'')
            || t.chars().skip(1).any(|c| c.is_ascii_uppercase())
            || t.chars().any(|c| c.is_ascii_digit()))
}

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

/// `<url=killReport...>` tag, leaving only this text, and in some locales the killword+colon glues to
/// the first name word with no space ("击杀：Lord Road" is one whitespace token), so the victim never
fn extract_kill_drops(text: &str) -> Option<(String, Option<String>)> {
    let lower = text.to_lowercase();
    let (kw_start, kw) = KILL_WORDS
        .iter()
        .filter_map(|kw| lower.find(kw).map(|i| (i, *kw)))
        .min_by_key(|&(i, _)| i)?;
    let rest = text.get(kw_start + kw.len()..)?;
    let name_start =
        rest.char_indices().find(|&(_, c)| c != ':' && c != '\u{FF1A}' && !c.is_whitespace())?.0;
    let rest = &rest[name_start..];
    let end = rest.find('(').unwrap_or(rest.len());
    let words: Vec<&str> = rest[..end].split_whitespace().take(3).collect();
    if words.is_empty() {
        return None;
    }
    let victim = words.join(" ");
    let ship = rest
        .get(end..)
        .and_then(|s| s.strip_prefix('('))
        .and_then(|s| s.split(')').next())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    Some((victim, ship))
}

fn segment_is_name(seg: &str, systems: &Systems) -> bool {
    let words: Vec<&str> = seg.split_whitespace().collect();
    if words.is_empty()
        || seg.chars().filter(|c| !c.is_whitespace()).count() < 3
        || !seg.chars().any(|c| c.is_alphabetic())
    {
        return false;
    }
    let bad_keyword =
        |w: &str| is_pilot_stopword(w) && !is_name_connector(w) && !is_name_capable_stopword(w);
    words.iter().any(|w| !is_pilot_stopword(w) && resolve(systems, w).is_none())
        && words.iter().filter(|w| resolve(systems, w).is_some()).count() * 2 <= words.len()
        && words.iter().filter(|w| bad_keyword(w)).count() <= 1
}

fn trim_paste_location_tail(seg: &str, ship_index: &HashMap<String, (i64, String)>) -> String {
    const LOC_PREP: &[&str] = &["at", "in", "on", "near"];
    let mut words: Vec<&str> = seg.split_whitespace().collect();
    while words.len() >= 2 && is_decorated_count(words[words.len() - 1]) {
        words.pop();
    }
    while words.len() >= 2 && {
        let last = words[words.len() - 1];
        (is_pilot_stopword(last)
            && !is_name_connector(last)
            && !is_name_capable_stopword(last)
            && !is_name_suffix(last))
            || ship_of(&last.to_lowercase(), ship_index).is_some()
    } {
        words.pop();
    }
    match words
        .iter()
        .enumerate()
        .find(|(i, w)| *i >= 2 && LOC_PREP.contains(&w.to_lowercase().as_str()))
        .map(|(i, _)| i)
    {
        Some(cut) => words[..cut].join(" "),
        None => words.join(" "),
    }
}

fn is_decorated_count(w: &str) -> bool {
    let t = w.trim();
    let decorated = t.starts_with('+')
        || t.starts_with(['x', 'X'])
        || t.ends_with('+')
        || t.ends_with(['x', 'X']);
    decorated
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().all(|c| c.is_ascii_digit() || matches!(c, '+' | 'x' | 'X'))
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

fn known_name_in_system_run(
    run: &str,
    known: &std::collections::HashMap<String, i64>,
    systems: &Systems,
) -> Option<String> {
    if known.is_empty() {
        return None;
    }
    let words: Vec<&str> = run.split_whitespace().collect();
    let n = words.len();
    for len in (1..=n).rev() {
        for start in 0..=n - len {
            if len == n {
                continue;
            }
            let span = words[start..start + len].join(" ");
            if !known.contains_key(&span.to_lowercase())
                || span.split_whitespace().all(is_pilot_stopword)
            {
                continue;
            }
            let rest_all_systems = words
                .iter()
                .enumerate()
                .filter(|(i, _)| *i < start || *i >= start + len)
                .all(|(_, w)| is_system_token(w, systems));
            if rest_all_systems {
                return Some(span);
            }
        }
    }
    None
}

fn run_covered_by_pilots(run: &str, pilots: &[String], systems: &Systems) -> bool {
    let existing: Vec<Vec<String>> = pilots
        .iter()
        .map(|p| p.split_whitespace().map(|w| w.to_lowercase()).collect())
        .collect();
    let words: Vec<String> = run.split_whitespace().map(|w| w.to_lowercase()).collect();
    let mut i = 0;
    let mut matched_any = false;
    while i < words.len() {
        if is_system_token(&words[i], systems) {
            i += 1;
            continue;
        }
        let adv = existing
            .iter()
            .filter(|c| !c.is_empty() && i + c.len() <= words.len() && words[i..i + c.len()] == c[..])
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        if adv == 0 {
            return false;
        }
        matched_any = true;
        i += adv;
    }
    matched_any
}

fn is_name_connector(w: &str) -> bool {
    matches!(
        w.to_lowercase().as_str(),
        "the" | "of" | "and" | "for" | "von" | "van" | "de" | "del" | "di" | "da"
            | "la" | "le" | "el" | "der" | "den" | "du" | "lord"
    )
}

fn is_name_capable_stopword(w: &str) -> bool {
    matches!(
        w.to_lowercase().as_str(),
        "blue" | "blues" | "red" | "reds" | "bubble" | "bubbles" | "clear" | "autopilot"
    )
}

fn is_name_suffix(t: &str) -> bool {
    (t.len() == 1 && t.starts_with(|c: char| c.is_ascii_uppercase()) && t != "I")
        || (matches!(t.len(), 1..=4) && t.chars().all(|c| c.is_ascii_digit()))
        || (t.starts_with('-')
            && matches!(t.len(), 2..=4)
            && t[1..].chars().all(|c| c.is_ascii_alphanumeric()))
}

fn extract_pilots(text: &str) -> Vec<String> {
    let is_namepart = name_part;
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, out: &mut Vec<String>| {
        if (2..=3).contains(&run.len())
            && run.iter().any(|w| !is_pilot_stopword(w))
            && !run
                .iter()
                .any(|w| is_pilot_stopword(w) && !is_name_connector(w) && !is_name_capable_stopword(w))
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

fn ship_of<'a>(
    lc: &str,
    ship_index: &'a HashMap<String, (i64, String)>,
) -> Option<&'a (i64, String)> {
    ship_index
        .get(lc)
        .or_else(|| lc.strip_suffix("ies").and_then(|base| ship_index.get(&format!("{base}y"))))
        .or_else(|| lc.strip_suffix("es").filter(|s| s.len() >= 3).and_then(|s| ship_index.get(s)))
        .or_else(|| lc.strip_suffix('s').filter(|s| s.len() >= 3).and_then(|s| ship_index.get(s)))
}

fn hard_name_breaker(core: &str, ship_index: &HashMap<String, (i64, String)>) -> bool {
    let lc = core.to_lowercase();
    core.is_empty()
        || !core.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
        || is_cap_word(&lc)
        || is_tackle_word(&lc)
        || is_time_token(core)
        || is_distance_token(core)
        || is_structure_word(core)
        || crate::wormholes::is_wh_code(core)
        || ship_of(&lc, ship_index).is_some()
}

fn is_distance_token(t: &str) -> bool {
    let lower = t.to_lowercase();
    let Some(de) = lower.find(|c: char| !(c.is_ascii_digit() || c == '.')) else {
        return false;
    };
    if de == 0 {
        return false;
    }
    matches!(&lower[de..], "km" | "au")
}

pub(crate) fn has_held_system(report: &IntelReport, systems: &Systems) -> bool {
    report
        .pilots
        .iter()
        .flat_map(|p| p.split_whitespace())
        .any(|w| is_system_token(w, systems))
}

fn is_system_token(core: &str, systems: &Systems) -> bool {
    (looks_like_system_code(core) && !is_code_lookalike_name(core, systems))
        || systems.lookup(core).is_some()
}

fn is_name_anchor(core: &str, ship_index: &HashMap<String, (i64, String)>, systems: &Systems) -> bool {
    !hard_name_breaker(core, ship_index)
        && !is_system_token(core, systems)
        && !is_pilot_stopword(core)
        && core.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
}

fn loose_pilot_runs(
    text: &str,
    ship_index: &HashMap<String, (i64, String)>,
    systems: &Systems,
) -> Vec<String> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, out: &mut Vec<String>| {
        let trim = |w: &String| {
            !is_name_suffix(w)
                && ((is_pilot_stopword(w) && w.chars().count() <= 3) || w.chars().count() < 2)
        };
        while run.first().is_some_and(&trim) {
            run.remove(0);
        }
        while run.last().is_some_and(&trim) {
            run.pop();
        }
        let letters: usize = run.iter().map(|w| w.chars().filter(|c| c.is_alphabetic()).count()).sum();
        let has_capital = run.iter().any(|w| name_part(w));
        let all_stop = run.iter().all(|w| is_pilot_stopword(w));
        if (2..=20).contains(&run.len()) && letters >= 3 && (has_capital || !all_stop) {
            let name = run.join(" ");
            if !out.contains(&name) {
                out.push(name);
            }
        }
        run.clear();
    };
    let is_strong_name = |w: &str| {
        name_part(w) && w.chars().any(|c| c.is_ascii_lowercase()) && !is_pilot_stopword(w)
    };
    let toks: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
    for (i, core) in toks.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|j| toks.get(j));
        let next = toks.get(i + 1);
        let breaks = if hard_name_breaker(core, ship_index) {
            true
        } else if core.chars().count() == 1
            && core.chars().all(|c| c.is_ascii_alphabetic())
            && !is_name_suffix(core)
            && !is_system_token(core, systems)
        {
            true
        } else if is_pilot_stopword(core)
            && !is_name_connector(core)
            && !is_name_capable_stopword(core)
            && !name_part(core)
            && prev.is_some_and(|w| is_strong_name(w))
            && next.is_some_and(|w| is_strong_name(w))
        {
            true
        } else if is_system_token(core, systems) {
            ![prev, next].into_iter().flatten().any(|n| is_name_anchor(n, ship_index, systems))
        } else {
            false
        };
        if breaks {
            flush(&mut run, &mut out);
        } else {
            run.push((*core).to_owned());
        }
    }
    flush(&mut run, &mut out);
    out
}

fn multiword_ships(
    text: &str,
    ship_index: &HashMap<String, (i64, String)>,
    known_pilots: &HashMap<String, i64>,
) -> Vec<(usize, usize, i64, String)> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
    let multi: Vec<(i64, &str, Vec<&str>)> = ship_index
        .iter()
        .filter_map(|(k, (id, name))| {
            let w: Vec<&str> = k.split_whitespace().collect();
            (2..=4).contains(&w.len()).then_some((*id, name.as_str(), w))
        })
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut adv = 1;
        let max = 4.min(words.len() - i);
        let mut matched = false;
        for len in (2..=max).rev() {
            let phrase = words[i..i + len].join(" ").to_lowercase();
            let try_phrase = |p: &str| -> Option<(i64, String)> {
                if let Some((id, name)) = ship_index.get(p) {
                    return Some((*id, name.clone()));
                }
                let full = if p.ends_with(" navy") || p.ends_with(" fleet") {
                    Some(format!("{p} issue"))
                } else if let Some(base) = p.strip_suffix(" ni") {
                    Some(format!("{base} navy issue"))
                } else if let Some(base) = p.strip_suffix(" fi") {
                    Some(format!("{base} fleet issue"))
                } else {
                    None
                };
                full.and_then(|f| ship_index.get(&f).map(|(id, name)| (*id, name.clone())))
            };
            let hit = try_phrase(&phrase).or_else(|| {
                [
                    phrase.strip_suffix("ies").map(|b| format!("{b}y")),
                    phrase.strip_suffix("es").map(str::to_owned),
                    phrase.strip_suffix('s').map(str::to_owned),
                ]
                .into_iter()
                .flatten()
                .filter(|s| s.split_whitespace().last().is_some_and(|w| w.len() >= 3))
                .find_map(|s| try_phrase(&s))
            });
            if let Some((id, name)) = hit {
                out.push((i, len, id, name));
                adv = len;
                matched = true;
                break;
            }
        }
        if !matched {
            for len in (2..=max).rev() {
                let win: Vec<String> =
                    words[i..i + len].iter().map(|w| w.to_lowercase()).collect();
                if known_pilots.contains_key(&win.join(" ")) {
                    continue;
                }
                let mut hit: Option<(i64, String)> = None;
                let mut ambiguous = false;
                for (id, name, hw) in &multi {
                    if hw.len() != win.len() {
                        continue;
                    }
                    let mut diffs = 0u32;
                    let mut ok = true;
                    for (a, b) in win.iter().zip(hw.iter()) {
                        if a == *b {
                            continue;
                        }
                        diffs += 1;
                        let (la, lb) = (a.chars().count(), b.chars().count());
                        if diffs > 1
                            || la.min(lb) < 4
                            || la.max(lb) < 5
                            || crate::shipnames::edit_distance(a, b) > 1
                        {
                            ok = false;
                            break;
                        }
                    }
                    if ok && diffs == 1 {
                        match &hit {
                            Some((hid, _)) if *hid != *id => {
                                ambiguous = true;
                                break;
                            }
                            _ => hit = Some((*id, (*name).to_string())),
                        }
                    }
                }
                if let (Some((id, name)), false) = (hit, ambiguous) {
                    out.push((i, len, id, name));
                    adv = len;
                    break;
                }
            }
        }
        i += adv;
    }
    out
}

fn multiword_systems(text: &str, systems: &Systems) -> Vec<(usize, usize, i64, String)> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
    let mut out: Vec<(usize, usize, i64, String)> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut adv = 1;
        let maxlen = 4.min(words.len() - i);
        for len in (2..=maxlen).rev() {
            let phrase = words[i..i + len].join(" ");
            if let Some(info) = systems.lookup(&phrase) {
                out.push((i, len, info.id, info.name.clone()));
                adv = len;
                break;
            }
        }
        i += adv;
    }
    out
}

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

fn lowercase_lead_system_names(
    text: &str,
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
) -> Vec<String> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> =
        text.split_whitespace().map(|w| w.trim_matches(punct)).filter(|w| !w.is_empty()).collect();
    let sys_count = words
        .iter()
        .filter(|w| resolve(systems, w).is_some() || looks_like_system_code(w))
        .count();
    if sys_count < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for w in words.windows(2) {
        let (a, b) = (w[0], w[1]);
        let a_lc = a.to_lowercase();
        let a_ok = a.chars().count() >= 3
            && a.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && a.chars().all(|c| c.is_ascii_alphabetic() || c == '\'' || c == '-')
            && !is_pilot_stopword(a)
            && !CLEAR_WORDS.contains(&a_lc.as_str())
            && resolve(systems, a).is_none()
            && !ship_index.contains_key(&a_lc);
        let b_ok = name_part(b) && !is_pilot_stopword(b) && resolve(systems, b).is_some();
        if a_ok && b_ok {
            out.push(format!("{a} {b}"));
        }
    }
    out
}

fn drop_subphrase_pilots(
    pilots: &mut Vec<String>,
    protect: &std::collections::HashSet<String>,
    source: &str,
) {
    let lc: Vec<String> = pilots.iter().map(|p| p.to_lowercase()).collect();
    let toks: Vec<Vec<String>> =
        pilots.iter().map(|p| tokenize(p).iter().map(|t| t.to_lowercase()).collect()).collect();
    let src: Vec<String> = tokenize(source).iter().map(|t| t.to_lowercase()).collect();
    fn count_seq(hay: &[String], needle: &[String]) -> usize {
        if needle.is_empty() || needle.len() > hay.len() {
            return 0;
        }
        let mut n = 0;
        let mut i = 0;
        while i + needle.len() <= hay.len() {
            if hay[i..i + needle.len()] == *needle {
                n += 1;
                i += needle.len();
            } else {
                i += 1;
            }
        }
        n
    }
    let keep: Vec<bool> = (0..pilots.len())
        .map(|i| {
            if protect.contains(&lc[i]) {
                return true;
            }
            let longer: Vec<usize> = (0..pilots.len())
                .filter(|&j| {
                    j != i
                        && lc[j].len() > lc[i].len()
                        && format!(" {} ", lc[j]).contains(&format!(" {} ", lc[i]))
                })
                .collect();
            if longer.is_empty() {
                return true;
            }
            let total = count_seq(&src, &toks[i]);
            let consumed: usize = longer
                .iter()
                .map(|&j| count_seq(&src, &toks[j]) * count_seq(&toks[j], &toks[i]))
                .sum();
            total > consumed
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
    t.replace('*', "")
}

#[derive(Default)]
pub struct Sightings {
    map: HashMap<String, Vec<(i64, i64)>>,
}

const SIGHTINGS_WINDOW: i64 = 14400;

pub type SharedSightings = std::sync::Arc<std::sync::Mutex<Sightings>>;

impl Sightings {
    pub fn record(&mut self, name: &str, system_id: i64, ts: i64) {
        if system_id <= 0 {
            return;
        }
        self.map.entry(name.to_lowercase()).or_default().push((system_id, ts));
    }

    pub fn prune(&mut self, now: i64) {
        let cutoff = now - SIGHTINGS_WINDOW;
        self.map.retain(|_, v| {
            v.retain(|&(_, ts)| ts >= cutoff);
            !v.is_empty()
        });
    }

    #[allow(dead_code)]
    pub fn distinct_systems_since(&self, name: &str, window_secs: i64, now: i64) -> usize {
        let cutoff = now - window_secs;
        let Some(v) = self.map.get(&name.to_lowercase()) else {
            return 0;
        };
        v.iter()
            .filter(|&&(_, ts)| ts >= cutoff)
            .map(|&(sys, _)| sys)
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    #[allow(dead_code)]
    pub fn revived(&self, name: &str, now: i64) -> bool {
        self.distinct_systems_since(name, 3600, now) >= 3
            || self.distinct_systems_since(name, SIGHTINGS_WINDOW, now) >= 5
    }
}

#[allow(dead_code)]
pub fn analyze(
    text: &str,
    systems: &Systems,
    ship_index: &std::collections::HashMap<String, (i64, String)>,
    known_pilots: &std::collections::HashMap<String, i64>,
    received: i64,
    channel: &str,
    reporter: &str,
) -> IntelReport {
    analyze_ctx(
        text,
        systems,
        ship_index,
        known_pilots,
        received,
        channel,
        reporter,
        None,
        &[],
        &std::collections::HashSet::new(),
    )
}

/// Localised "Kill:" prefixes from the in-game killReport link text. EVE doesn't write
/// the `<url=killReport...>` wrapper to the chat log, so a kill is detected from the
/// visible (localised) word, not the URL.
const KILL_WORDS: &[&str] = &[
    "kill:",
    "击杀",
    "损失",
    "キル",
    "킬",
    "abschuss",
    "убийство",
];

pub fn parse_motd_regions(motd: &str, known: &std::collections::HashSet<String>) -> Vec<String> {
    let body = match motd.rfind("Channel MOTD:") {
        Some(i) => &motd[i + "Channel MOTD:".len()..],
        None => motd,
    };
    let bb = body.as_bytes();
    let mut hits: Vec<(usize, &String)> = Vec::new();
    for region in known {
        let r = region.as_bytes();
        if r.is_empty() {
            continue;
        }
        let mut at = 0;
        while at + r.len() <= bb.len() {
            if bb[at..at + r.len()].eq_ignore_ascii_case(r) {
                let before_ok = at == 0 || !(bb[at - 1] as char).is_ascii_alphabetic();
                let after = at + r.len();
                let after_ok = after >= bb.len() || !(bb[after] as char).is_ascii_lowercase();
                if before_ok && after_ok {
                    hits.push((at, region));
                    break;
                }
            }
            at += 1;
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn detect_location(
    tokens: &[&str],
    lower_tokens: &[String],
    reserved: &std::collections::HashSet<String>,
    systems: &Systems,
    context_system: Option<i64>,
    channel_regions: &[String],
) -> (Vec<DetectedSystem>, Vec<String>, Vec<String>) {
    let pilot_tokens = reserved;
    let mut detected: Vec<DetectedSystem> = Vec::new();
    let mut consumed: Vec<String> = Vec::new();
    let mut deferred: Vec<&str> = Vec::new();
    for tok in tokens {
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

    {
        let primary = detected.first().map(|d| d.id).or(context_system);
        if let Some(p) = primary {
            for tok in tokens.iter() {
                let lc = tok.to_lowercase();
                if consumed.contains(&lc)
                    || looks_like_system_code(tok)
                    || !is_short_code_token(tok)
                    || resolve(systems, tok).is_some()
                {
                    continue;
                }
                let hit = systems.neighbors(p).iter().find_map(|&nid| {
                    systems.info_of(nid).filter(|info| info.name.to_lowercase().starts_with(&lc))
                });
                if let Some(info) = hit {
                    let (id, name, security) = (info.id, info.name.clone(), info.security);
                    consumed.push(lc);
                    if !detected.iter().any(|d| d.id == id) {
                        detected.push(DetectedSystem { id, name, security });
                    }
                }
            }
        }
    }

    let mut gate: Option<String> = None;
    let primary = detected.first().map(|d| d.id).or(context_system);
    let is_gate_word =
        |t: &str| matches!(t.to_lowercase().as_str(), "gate" | "gates" | "stargate" | "stargates");
    for (i, tok) in tokens.iter().enumerate() {
        if !is_gate_word(tok) || i == 0 {
            continue;
        }
        let cand = tokens[i - 1];
        if cand.eq_ignore_ascii_case("on") || cand.eq_ignore_ascii_case("the") {
            continue;
        }
        let is_name_surname = pilot_tokens.contains(&cand.to_lowercase())
            && resolve(systems, cand).is_some()
            && i >= 2
            && tokens.get(i - 2).is_some_and(|p| {
                let pl = p.to_lowercase();
                pilot_tokens.contains(&pl)
                    && !is_system_token(p, systems)
                    && !is_pilot_stopword(p)
                    && !looks_like_system_code(p)
                    && p.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
            });
        if is_name_surname {
            continue;
        }
        let resolved = resolve(systems, cand)
            .or_else(|| {
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
                let abbrev = cand.len() >= 2
                    && cand.chars().all(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || c == '-');
                if abbrev { systems.lookup_prefix(cand) } else { None }
            });
        if resolved.is_some()
            && resolved.map(|s| s.id) == primary
            && tokens.get(i + 1).is_some_and(|n| n.eq_ignore_ascii_case("camp"))
        {
            continue;
        }
        if resolved.is_none() && cand.chars().all(|c| c.is_ascii_digit()) {
            break;
        }
        match resolved {
            Some(info) => {
                gate = Some(info.name.clone());
                consumed.push(cand.to_lowercase());
                detected.retain(|d| d.id != info.id);
            }
            None => {
                gate = Some(
                    primary
                        .map(|p| systems.neighbors_gates_only(p))
                        .filter(|ns| ns.len() == 1)
                        .and_then(|ns| systems.info_of(ns[0]))
                        .map(|s| s.name.clone())
                        .unwrap_or_default(),
                );
            }
        }
        break;
    }

    if gate.is_none() && lower_tokens.iter().any(|t| t == "ansi" || t == "ansiblex") {
        if let Some(dest) = primary.and_then(|p| systems.jump_bridge_dest(p)) {
            detected.retain(|d| d.id != dest.id);
            gate = Some(dest.name.clone());
        }
    }

    if detected.len() > 1 {
        let wh_word = |w: &str| {
            matches!(w.to_lowercase().as_str(), "hole" | "holes" | "wh" | "wormhole")
        };
        let is_wh_ref = |sys: &DetectedSystem| -> bool {
            let name_lc = sys.name.to_lowercase();
            if name_lc == "thera" || name_lc == "turnur" {
                return true;
            }
            tokens.iter().enumerate().any(|(i, t)| {
                resolve(systems, t).map(|info| info.id) == Some(sys.id)
                    && (tokens.get(i + 1).is_some_and(|n| wh_word(n))
                        || i.checked_sub(1)
                            .and_then(|j| tokens.get(j))
                            .is_some_and(|p| p.eq_ignore_ascii_case("to")))
            })
        };
        if detected.iter().any(|d| !is_wh_ref(d)) {
            detected.retain(|d| !is_wh_ref(d));
        }
    }

    let mut gates: Vec<String> = Vec::new();
    if let Some(g) = gate {
        gates.push(g);
    }
    if detected.len() > 1 {
        let primary = detected[0].id;
        let adjacent: std::collections::HashSet<i64> =
            systems.neighbors(primary).iter().copied().collect();
        for d in detected.split_off(1) {
            if !adjacent.is_empty() && !adjacent.contains(&d.id) {
                continue;
            }
            if !gates.iter().any(|g| g.eq_ignore_ascii_case(&d.name)) {
                gates.push(d.name);
            }
        }
    }
    (detected, gates, consumed)
}

#[allow(clippy::too_many_arguments)]
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
    denied: &std::collections::HashSet<String>,
) -> IntelReport {
    let cleaned = preprocess_intel(text);
    let text = cleaned.as_str();
    let display_text = text.trim().to_owned();
    let links = extract_links(text);
    let stripped = strip_urls(text);
    let text = stripped.as_str();
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = tokenize(text);
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();

    let mw_ships = multiword_ships(text, ship_index, known_pilots);
    let mw_systems = multiword_systems(text, systems);
    let cel_words = structure_words(text);
    let struct_spans = structure_spans(&cel_words);
    let belt_spans = belt_locations(&cel_words);
    let masked_words: String = {
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut start: Option<usize> = None;
        for (i, c) in text.char_indices() {
            if c.is_whitespace() {
                if let Some(s) = start.take() {
                    spans.push((s, i));
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        if let Some(s) = start {
            spans.push((s, text.len()));
        }
        let mut blank: Vec<(usize, usize)> = Vec::new();
        for (w, len, _, _) in mw_ships.iter().chain(mw_systems.iter()) {
            for k in *w..(*w + *len).min(spans.len()) {
                blank.push(spans[k]);
            }
        }
        for (w, len, _) in struct_spans.iter().chain(belt_spans.iter()) {
            for k in *w..(*w + *len).min(spans.len()) {
                blank.push(spans[k]);
            }
        }
        text.char_indices()
            .map(|(i, c)| if blank.iter().any(|(s, e)| i >= *s && i < *e) { ' ' } else { c })
            .collect()
    };
    let masked = mask_parens(&masked_words);
    let mut pilots = extract_pilots(&masked);
    for n in lowercase_tail_names(&masked, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    for n in numbered_names(&tokenize(&masked)) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    for k in match_known_pilots(&masked, known_pilots) {
        if denied.contains(&k.to_lowercase()) {
            continue;
        }
        if (!k.contains(' ') && ship_index.contains_key(&k.to_lowercase()))
            || is_system_token(&k, systems)
            || is_time_token(&k)
            || is_structure_word(&k)
            || (!k.contains(' ')
                && systems.lookup_prefix(&k).is_some_and(|s| looks_like_system_code(&s.name)))
        {
            continue;
        }
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&k)) {
            pilots.push(k);
        }
    }
    for n in lowercase_known_compound(&masked, known_pilots, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    for n in lowercase_lead_system_names(&masked, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    let mut drop_ships: Vec<(i64, String)> = Vec::new();
    for (pilot, ship_text) in extract_dscan_drops(text) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&pilot)) {
            pilots.push(pilot);
        }
        if let Some((id, name)) = ship_index.get(&ship_text.to_lowercase()) {
            drop_ships.push((*id, name.clone()));
        }
    }
    let quoted_raw = extract_quoted(text);
    let quoted: std::collections::HashSet<String> =
        quoted_raw.iter().map(|q| q.to_lowercase()).collect();
    for q in quoted_raw {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&q)) {
            pilots.push(q);
        }
    }
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
    let lc: Vec<String> = pilots.iter().map(|p| p.to_lowercase()).collect();
    pilots = pilots
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            let me = &lc[*i];
            let is_subphrase = lc.iter().enumerate().any(|(j, other)| {
                j != *i && other.len() > me.len() && format!(" {other} ").contains(&format!(" {me} "))
            });
            let is_ship_name = ship_index.contains_key(me);
            let single_stop = !p.contains(' ')
                && !quoted.contains(me)
                && (PILOT_STOP.contains(&me.as_str()) || CLEAR_WORDS.contains(&me.as_str()));
            !is_subphrase && !is_ship_name && !single_stop
        })
        .map(|(_, p)| p.clone())
        .collect();
    pilots.retain(|p| !p.split_whitespace().any(crate::wormholes::is_wh_code));
    for r in loose_pilot_runs(&masked, ship_index, systems) {
        if pilots.iter().any(|p| p.eq_ignore_ascii_case(&r)) {
            continue;
        }
        if is_pilot_stopword(&r) {
            continue;
        }
        if denied.contains(&r.to_lowercase()) {
            continue;
        }
        if run_covered_by_pilots(&r, &pilots, systems) {
            continue;
        }
        if let Some(known_name) = known_name_in_system_run(&r, known_pilots, systems) {
            if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&known_name)) {
                pilots.push(known_name);
            }
            continue;
        }
        pilots.push(r);
    }
    if let Some((victim, ship_text)) = extract_kill_drops(text) {
        let kill_prefixes: Vec<String> = KILL_WORDS
            .iter()
            .map(|k| k.trim_end_matches([':', '：']).to_lowercase())
            .filter(|k| !k.is_empty())
            .collect();
        let victim_words: Vec<String> =
            victim.split_whitespace().map(|w| w.to_lowercase()).collect();
        pilots.retain(|p| {
            let pw: Vec<String> = p.split_whitespace().map(|w| w.to_lowercase()).collect();
            !(pw.len() == victim_words.len() + 1
                && kill_prefixes.contains(&pw[0])
                && pw[1..] == victim_words[..])
        });
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&victim)) {
            pilots.push(victim);
        }
        if let Some((id, name)) = ship_text.and_then(|s| ship_index.get(&s.to_lowercase())) {
            drop_ships.push((*id, name.clone()));
        }
    }
    let masked_tokens = tokenize(&masked);
    for t in &masked_tokens {
        let lc = t.to_lowercase();
        let name_word = t.chars().count() >= 3
            && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
            && t.chars().any(|c| c.is_ascii_alphabetic());
        if name_word
            && !is_pilot_stopword(t)
            && !denied.contains(&lc)
            && !is_cap_word(&lc)
            && !is_tackle_word(&lc)
            && !is_time_token(t)
            && !is_distance_token(t)
            && !is_amount_token(t)
            && (!looks_like_system_code(t) || is_code_lookalike_name(t, systems))
            && !CLEAR_WORDS.contains(&lc.as_str())
            && ship_index.get(&lc).is_none()
            && resolve(systems, t).is_none()
            && !crate::wormholes::is_wh_code(t)
            && !pilots.iter().any(|p| p.split_whitespace().any(|w| w.eq_ignore_ascii_case(t)))
        {
            pilots.push((*t).to_owned());
        }
    }
    pilots.retain(|p| !is_structure_word(p));
    let mut paste_origin: std::collections::HashSet<String> = std::collections::HashSet::new();
    if text.contains("  ") {
        let segments: Vec<&str> =
            text.split("  ").map(str::trim).filter(|s| !s.is_empty()).collect();
        let names: Option<Vec<&str>> = (segments.len() > 1)
            .then(|| {
                let mut names = Vec::new();
                let mut anchor = false;
                for seg in &segments {
                    let seg_words: Vec<&str> = seg.split_whitespace().collect();
                    if (seg_words.len() > 1 && seg_words.first().is_some_and(|w| is_decorated_count(w)))
                        || (seg_words.len() > 1
                            && seg_words.iter().all(|w| ship_of(&w.to_lowercase(), ship_index).is_some()))
                    {
                        return None;
                    }
                    let is_mention = |w: &str| {
                        let wl = w.to_lowercase();
                        resolve(systems, w).is_some()
                            || systems.lookup_prefix(&wl).is_some()
                            || ship_of(&wl, ship_index).is_some()
                            || is_structure_word(w)
                            || crate::wormholes::is_wh_code(w)
                            || (looks_like_system_code(w) && !is_code_lookalike_name(w, systems))
                    };
                    let confirmed_system = |w: &str| {
                        looks_like_system_code(w)
                            && (resolve(systems, w).is_some()
                                || systems.lookup_prefix(&w.to_lowercase()).is_some())
                    };
                    if is_mention(seg) || seg.split_whitespace().any(confirmed_system) {
                        anchor = true;
                        continue;
                    }
                    if segment_is_name(seg, systems) {
                        names.push(*seg);
                    } else if seg.split_whitespace().any(is_mention) {
                        anchor = true;
                    } else {
                        return None;
                    }
                }
                (anchor && !names.is_empty()).then_some(names)
            })
            .flatten();
        if let Some(names) = names {
            let seg_padded: Vec<String> =
                segments.iter().map(|s| format!(" {} ", s.to_lowercase())).collect();
            pilots.retain(|p| {
                !p.contains(' ')
                    || seg_padded.iter().any(|s| s.contains(&format!(" {} ", p.to_lowercase())))
            });
            for seg in names {
                let name = trim_paste_location_tail(seg, ship_index);
                paste_origin.insert(name.to_lowercase());
                if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&name)) {
                    pilots.push(name);
                }
            }
        }
    }
    {
        let lc: Vec<String> = pilots.iter().map(|p| p.to_lowercase()).collect();
        let covered_by_longer = |frag_words: usize, frag: &str, exclude: usize| -> bool {
            let needle = format!(" {frag} ");
            lc.iter().enumerate().any(|(j, other)| {
                j != exclude
                    && other.split_whitespace().count() > frag_words
                    && format!(" {other} ").contains(&needle)
            })
        };
        let mut kill = vec![false; pilots.len()];
        for (i, p) in pilots.iter().enumerate() {
            let words: Vec<&str> = p.split_whitespace().collect();
            if words.len() < 2 {
                continue;
            }
            let rem = if ship_index.contains_key(&words[words.len() - 1].to_lowercase()) {
                Some(words[..words.len() - 1].join(" "))
            } else if ship_index.contains_key(&words[0].to_lowercase()) {
                Some(words[1..].join(" "))
            } else {
                None
            };
            if let Some(rem) = rem {
                let rem_lc = rem.to_lowercase();
                if !rem_lc.is_empty()
                    && covered_by_longer(rem.split_whitespace().count(), &rem_lc, i)
                {
                    kill[i] = true;
                }
            }
        }
        let mut it = kill.iter();
        pilots.retain(|_| !it.next().copied().unwrap_or(false));
    }
    // Double-consume guard: a source word claimed by one pilot must not be re-used by another. When
    // consumed can't seed a bogus second pilot. Positions decide this, never letter case.
    {
        let src: Vec<String> = tokenize(text).iter().map(|t| t.to_lowercase()).collect();
        let span = |p: &str| -> Option<(usize, usize)> {
            let cw: Vec<String> = tokenize(p).iter().map(|t| t.to_lowercase()).collect();
            if cw.is_empty() || cw.len() > src.len() {
                return None;
            }
            (0..=src.len() - cw.len()).find(|&i| src[i..i + cw.len()] == cw[..]).map(|i| (i, i + cw.len()))
        };
        let spans: Vec<Option<(usize, usize)>> = pilots.iter().map(|p| span(p)).collect();
        let stronger = |i: usize, j: usize| -> bool {
            let ki = known_pilots.contains_key(&pilots[i].to_lowercase());
            let kj = known_pilots.contains_key(&pilots[j].to_lowercase());
            if ki != kj {
                return ki;
            }
            let wi = spans[i].map(|(s, e)| e - s).unwrap_or(0);
            let wj = spans[j].map(|(s, e)| e - s).unwrap_or(0);
            if wi != wj {
                return wi > wj;
            }
            spans[i].map(|(s, _)| s).unwrap_or(usize::MAX) < spans[j].map(|(s, _)| s).unwrap_or(usize::MAX)
        };
        let partial = |a: (usize, usize), b: (usize, usize)| -> bool {
            let intersect = a.0.max(b.0) < a.1.min(b.1);
            let a_in_b = b.0 <= a.0 && a.1 <= b.1;
            let b_in_a = a.0 <= b.0 && b.1 <= a.1;
            intersect && !a_in_b && !b_in_a
        };
        let mut kill = vec![false; pilots.len()];
        for i in 0..pilots.len() {
            for j in (i + 1)..pilots.len() {
                if let (Some(a), Some(b)) = (spans[i], spans[j]) {
                    if partial(a, b) {
                        if stronger(i, j) {
                            kill[j] = true;
                        } else {
                            kill[i] = true;
                        }
                    }
                }
            }
        }
        let mut it = kill.iter();
        pilots.retain(|_| !it.next().copied().unwrap_or(false));
    }
    drop_subphrase_pilots(&mut pilots, &std::collections::HashSet::new(), text);

    pilots.retain(|p| {
        p.contains(' ')
            || !is_lowercaseish(p)
            || quoted.contains(&p.to_lowercase())
            || paste_origin.contains(&p.to_lowercase())
            || !crate::dict::is_word(p)
    });

    let is_strong_name_word = |w: &str| {
        name_part(w) && w.chars().any(|c| c.is_ascii_lowercase()) && !is_pilot_stopword(w)
    };
    let mut pilot_tokens: std::collections::HashSet<String> = pilots
        .iter()
        .filter(|n| n.split_whitespace().any(|w| is_strong_name_word(w)))
        .flat_map(|n| n.split_whitespace())
        .map(|w| w.to_lowercase())
        .collect();
    for name in KEYWORD_NAME_PILOTS {
        if display_text.contains(name) {
            pilot_tokens.extend(name.split_whitespace().map(|w| w.to_lowercase()));
        }
    }
    let pilot_span_tokens: std::collections::HashSet<String> = pilots
        .iter()
        .filter(|n| n.split_whitespace().count() > 1)
        .flat_map(|n| n.split_whitespace())
        .map(|w| w.to_lowercase())
        .collect();

    let wh_code =
        tokens.iter().find(|t| crate::wormholes::is_wh_code(t)).map(|t| t.to_uppercase());

    let is_wh_msg = lower.contains("wormhole")
        || wh_code.is_some()
        || lower_tokens.iter().any(|t| {
            matches!(t.as_str(), "wh" | "hole" | "holes" | "thera" | "turnur")
                && !pilot_tokens.contains(t)
        });
    let (wh_dest, wh_size, wh_eol, wh_drifter, wh_sig) = if is_wh_msg {
        (
            parse_wh_dest(&lower, &lower_tokens),
            parse_wh_size(&lower, &lower_tokens),
            lower.contains("eol") || lower.contains("end of life") || lower.contains("dying"),
            lower.contains("drifter"),
            tokens.iter().find(|t| looks_like_sig(t)).map(|t| t.to_uppercase()),
        )
    } else {
        (None, None, false, false, None)
    };

    let mut ships: Vec<DetectedShip> = Vec::new();
    let add_ship = |id: i64, name: &str, ships: &mut Vec<DetectedShip>| {
        if !ships.iter().any(|s| s.id == id) {
            ships.push(DetectedShip { id, name: name.to_owned() });
        }
    };
    let mw_words: std::collections::HashSet<String> = {
        let punct = |c: char| ",.;:!?\"()".contains(c);
        let tw: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
        let mut s = std::collections::HashSet::new();
        for (start, len, _, name) in &mw_ships {
            for w in name.to_lowercase().split_whitespace() {
                s.insert(w.to_owned());
            }
            for w in tw.iter().skip(*start).take(*len) {
                s.insert(w.to_lowercase());
            }
        }
        s
    };
    for tok in &tokens {
        let lower = tok.to_lowercase();
        if pilot_tokens.contains(&lower)
            || mw_words.contains(&lower)
            || pilot_span_tokens.contains(&lower)
        {
            continue;
        }
        // "shuttle(s)" with no specific hull → default to the Caldari Shuttle (672).
        if matches!(lower.as_str(), "shuttle" | "shuttles") {
            add_ship(672, "Caldari Shuttle", &mut ships);
            continue;
        }
        if let Some((id, name)) = ship_of(&lower, ship_index) {
            add_ship(*id, name, &mut ships);
            continue;
        }
        if systems.lookup(tok).is_some() || known_pilots.contains_key(&lower) {
            continue;
        }
        if lower.is_ascii() && lower.len() >= 5 {
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
    for (id, name) in drop_ships {
        add_ship(id, &name, &mut ships);
    }
    for (_, _, id, name) in mw_ships {
        add_ship(id, &name, &mut ships);
    }

    let name_tokens: std::collections::HashSet<String> =
        pilots.iter().flat_map(|p| p.split_whitespace()).map(|w| w.to_lowercase()).collect();
    let (mut detected, gates, mut consumed) = detect_location(
        &tokens, &lower_tokens, &name_tokens, systems, context_system, channel_regions,
    );
    {
        let punct = |c: char| ",.;:!?\"()".contains(c);
        let sys_words: Vec<&str> =
            text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
        for (start, len, id, name) in &mw_systems {
            if !detected.iter().any(|d| d.id == *id) {
                let security = systems.info_of(*id).map_or(0.0, |i| i.security);
                detected.push(DetectedSystem { id: *id, name: name.clone(), security });
            }
            for w in sys_words.iter().skip(*start).take(*len) {
                consumed.push(w.to_lowercase());
            }
        }
    }

    let (diamond_rats, dia_consumed) = detect_diamond_rats(&tokens);
    let (anom_sigs, anom_consumed) = detect_anom_sigs(&tokens, systems);
    // A wormhole already shows its signature on the wormhole badge, so drop a duplicate Sig badge
    // for the same code.
    let anom_sigs: Vec<(AnomKind, String)> = anom_sigs
        .into_iter()
        .filter(|(_, code)| wh_sig.as_deref().map_or(true, |ws| !code.eq_ignore_ascii_case(ws)))
        .collect();
    let npc_consumed: std::collections::HashSet<String> =
        dia_consumed.into_iter().chain(anom_consumed).collect();
    consumed.extend(npc_consumed.iter().cloned());

    let mut alliances: Vec<(String, i64)> = Vec::new();
    for t in &lower_tokens {
        if let Some((name, id)) = crate::alliances::lookup(t) {
            if !alliances.iter().any(|(_, i)| *i == id) {
                alliances.push((name.to_owned(), id));
            }
        }
    }

    // A detected alliance name ("Shadow Cartel") must not also surface as pilots ("Shadow",
    // "Cartel"): drop pilot candidates whose every word is part of a matched alliance name.
    let pilots: Vec<String> = if alliances.is_empty() {
        pilots
    } else {
        let alliance_words: std::collections::HashSet<String> = alliances
            .iter()
            .flat_map(|(name, _)| name.split_whitespace().map(|w| w.to_lowercase()))
            .collect();
        pilots
            .into_iter()
            .filter(|p| !p.split_whitespace().all(|w| alliance_words.contains(&w.to_lowercase())))
            .collect()
    };

    let mut reclassified: Vec<DetectedShip> = Vec::new();
    let pilots: Vec<String> = pilots
        .into_iter()
        .filter(|pn| {
            let words: Vec<&str> = pn.split_whitespace().collect();
            if !words.is_empty() && words.iter().all(|w| ship_of(&w.to_lowercase(), ship_index).is_some()) {
                for w in &words {
                    if let Some((id, name)) = ship_of(&w.to_lowercase(), ship_index) {
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

    {
        let mut confirmed_tokens: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for p in &pilots {
            let lc = p.to_lowercase();
            if known_pilots.contains_key(&lc) || quoted.contains(&lc) {
                confirmed_tokens.extend(p.split_whitespace().map(|w| w.to_lowercase()));
            }
        }
        for w in pilots.iter().flat_map(|p| p.split_whitespace()) {
            let lw = w.to_lowercase();
            if known_pilots.contains_key(&lw) {
                confirmed_tokens.insert(lw);
            }
        }
        for p in &pilots {
            let words: Vec<&str> = p.split_whitespace().collect();
            if words.len() < 2 {
                continue;
            }
            if !words.iter().any(|w| confirmed_tokens.contains(&w.to_lowercase())) {
                continue;
            }
            for w in &words {
                let lw = w.to_lowercase();
                if confirmed_tokens.contains(&lw) || mw_words.contains(&lw) {
                    continue;
                }
                if let Some((id, name)) = ship_of(&lw, ship_index) {
                    add_ship(*id, name, &mut ships);
                }
            }
        }
    }

    let probe_text = {
        let mut t = std::borrow::Cow::Borrowed(text);
        for name in KEYWORD_NAME_PILOTS {
            if t.contains(name) {
                t = std::borrow::Cow::Owned(t.replace(name, &" ".repeat(name.len())));
            }
        }
        t
    };
    let probes = detect_probes(&probe_text);
    if probes.is_some() {
        ships.retain(|s| !s.name.eq_ignore_ascii_case("Probe"));
    }

    let classes = detect_classes(&lower_tokens, &pilot_tokens);
    let (mut tackled, tackled_targets) = detect_tackle(&lower_tokens, &pilot_tokens, ship_index);
    tackled |= lower.contains("抓") || lower.contains("点住") || lower.contains("网住");

    let raw_tokens: Vec<&str> = text.split_whitespace().collect();
    let (mut celestials, celestial_consumed) = detect_celestials(&raw_tokens);
    consumed.extend(celestial_consumed);
    for (start, len, label) in &belt_spans {
        if !celestials.iter().any(|c| c.eq_ignore_ascii_case(label)) {
            celestials.push(label.clone());
        }
        for w in cel_words.iter().skip(*start).take(*len) {
            consumed.push(w.clone());
        }
    }

    let mut pilots = drop_covered_prefixes(&pilots, text);
    pilots.retain(|p| p.chars().any(|c| c.is_alphabetic()));
    // Case and length don't decide a name: EVE names can be all-caps and short ("DT", "PORTOS11").
    pilots.retain(|p| p.contains(' ') || !consumed.contains(&p.to_lowercase()));
    let code_consumed: std::collections::HashSet<String> =
        consumed.iter().filter(|c| is_short_code_token(c)).cloned().collect();
    if !code_consumed.is_empty() {
        pilots = pilots
            .into_iter()
            .filter_map(|p| {
                if !p.contains(' ') {
                    return Some(p);
                }
                let kept: Vec<&str> = p
                    .split_whitespace()
                    .filter(|w| !code_consumed.contains(&w.to_lowercase()))
                    .collect();
                (!kept.is_empty()).then(|| kept.join(" "))
            })
            .collect();
    }
    {
        let mut seen = std::collections::HashSet::new();
        pilots.retain(|p| seen.insert(p.to_lowercase()));
    }
    for name in KEYWORD_NAME_PILOTS {
        if display_text.contains(name) && !pilots.iter().any(|p| p.eq_ignore_ascii_case(name)) {
            pilots.push((*name).to_string());
        }
    }
    if !npc_consumed.is_empty() {
        pilots = pilots
            .into_iter()
            .filter_map(|p| {
                let kept: Vec<&str> =
                    p.split_whitespace().filter(|w| !npc_consumed.contains(&w.to_lowercase())).collect();
                (!kept.is_empty()).then(|| kept.join(" "))
            })
            .collect();
    }
    let (total_count, plus_count, name_number_skips) =
        parse_count(text, &consumed, systems, ship_index, &pilots, known_pilots);
    let named = pilots.len() as u32;
    let solo = lower_tokens.iter().any(|t| t == "solo" && !pilot_tokens.contains(t));
    let count = derive_count(total_count, plus_count, 0, named, solo);
    let ess_ctx = lower_tokens.iter().any(|t| t == "ess" && !pilot_tokens.contains(t));
    let isk = parse_isk(text, ess_ctx);
    let structures = detect_structures(text);
    let mut report = IntelReport {
        id: 0,
        probes,
        received,
        channel: channel.to_owned(),
        reporter: reporter.to_owned(),
        text: display_text,
        pilots,
        systems: detected,
        ships,
        classes,
        count,
        count_extra: total_count,
        count_plus: plus_count,
        count_ships: 0,
        solo,
        name_number_skips,
        isk,
        structures,
        celestials,
        clear: !lower.contains('?')
            && lower_tokens
                .iter()
                .any(|t| CLEAR_WORDS.contains(&t.as_str()) && !pilot_tokens.contains(t)),
        status: lower_tokens
            .iter()
            .any(|t| matches!(t.as_str(), "status" | "stat" | "eyes") && !pilot_tokens.contains(t)),
        no_visual: lower_tokens.iter().any(|t| t == "nv" && !pilot_tokens.contains(t))
            || lower.contains("no visual"),
        spike: flagged(&lower_tokens, &pilot_tokens, &["spike"]),
        camp: flagged(&lower_tokens, &pilot_tokens, &["camp", "gatecamp", "camping", "camped", "gatecamping", "camper", "campers"]) || lower.contains("蹲"),
        help: flagged_exact(&lower_tokens, &pilot_tokens, &["help", "sos"])
            || lower.contains("need backup")
            || lower.contains("needs backup")
            || lower.contains("求救")
            || lower.contains("求助"),
        // Exact match, not prefix: the "drag" stem matched the destroyer Dragoon.
        bubble: flagged_exact(
            &lower_tokens,
            &pilot_tokens,
            &["bubble", "bubbles", "bubbled", "bubbling", "dragbubble", "drag", "drags"],
        ) || lower.contains("泡泡")
            || lower.contains("气泡"),
        // Token match (not `flagged_exact`): "nullified" is a capability note that often sits next
        // to the ship/pilot, so it must fire even when it lands inside a name run. It is a stop-word
        // so it never shows as a pilot itself.
        nullified: ["nullified", "nullifier", "nullifiers", "nullification", "nully", "nullie", "nullies"]
            .iter()
            .any(|w| lower_tokens.iter().any(|t| t == w)),
        killmail: links.iter().any(|l| l.kind == LinkKind::Killmail)
            || KILL_WORDS.iter().any(|w| lower.contains(w)),
        near_celestial: None,
        cyno: flagged_exact(
            &lower_tokens,
            &pilot_tokens,
            &["cyno", "cynos", "hotdrop", "hotdrops", "hotdropper", "hotdroppers"],
        ) || lower.contains("诱导")
            || lower.contains("诱饵")
            || lower.contains("hot drop"),
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
        wh_size,
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
        skyhook: lower.contains("skyhook") || lower_tokens.iter().any(|t| is_skyhook_typo(t)),
        filament: flagged_exact(
            &lower_tokens,
            &pilot_tokens,
            &["filament", "filaments", "needlejack", "needlejacks", "trace", "traces"],
        ),
        diamond_rats,
        anom_sigs,
        gates,
        alliances,
        movement: None,
        links,
    };
    // "Clear" loses to any sign of a threat: a contradictory message (a pilot named "clear …",
    // or "clear" next to real hostiles) must never downgrade severity. Prefer a false positive
    // (missed clear) over a false negative (missed threat).
    if report.clear
        && (report.cyno
            || report.dropper
            || report.bubble
            || report.camp
            || report.spike
            || report.killmail
            || report.cap_tackled
            || report.tackled
            || !report.ships.is_empty()
            || !report.pilots.is_empty()
            || report.count.unwrap_or(0) > 0)
    {
        report.clear = false;
    }
    report
}

fn parse_time_left(text: &str, max_min: u32) -> Option<String> {
    let toks: Vec<&str> = text.split_whitespace().collect();
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

fn is_cap_word(t: &str) -> bool {
    matches!(
        t,
        "cap" | "caps" | "capital" | "capitals" | "rorq" | "rorqs" | "rorqual" | "rorquals"
            | "dread" | "dreads" | "dreadnought" | "dreadnoughts" | "carrier" | "carriers"
            | "fax" | "faxes" | "titan" | "titans" | "super" | "supers" | "supercap"
            | "supercaps" | "supercarrier" | "supercarriers"
    )
}

fn is_tackle_word(t: &str) -> bool {
    t.starts_with("tackl")
        || t.starts_with("takl")
        || t.starts_with("tackel")
        || t.starts_with("scram")
        || t.starts_with("scrambl")
        || t.starts_with("point")
}

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

fn detect_cap_tackled(
    lower_tokens: &[String],
    pilot_tokens: &std::collections::HashSet<String>,
) -> bool {
    let cap = lower_tokens.iter().any(|t| !pilot_tokens.contains(t) && is_cap_word(t));
    let tackle = lower_tokens.iter().any(|t| !pilot_tokens.contains(t) && is_tackle_word(t));
    cap && tackle
}

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

fn is_short_number(t: &str) -> bool {
    (1..=2).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit())
}

fn resolve<'a>(systems: &'a Systems, token: &str) -> Option<&'a crate::geo::SystemInfo> {
    let first = token.chars().next()?;
    let proper = first.is_uppercase() || first.is_ascii_digit() || token.contains('-');
    if !proper {
        return None;
    }
    if let Some(info) = systems.lookup(token) {
        return Some(info);
    }
    if token.len() == 2 && token.chars().all(|c| c.is_ascii_digit()) {
        return systems.lookup_prefix(&format!("{token}-"));
    }
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

const STRUCTURES: &[(&str, &str)] = &[
    ("keepstar", "Keepstar"), ("keep", "Keepstar"), ("ks", "Keepstar"),
    ("fortizar", "Fortizar"), ("fort", "Fortizar"),
    ("astrahus", "Astrahus"), ("astra", "Astrahus"),
    ("raitaru", "Raitaru"), ("azbel", "Azbel"), ("sotiyo", "Sotiyo"),
    ("athanor", "Athanor"), ("tatara", "Tatara"),
    ("ansiblex", "Ansiblex"), ("ansi", "Ansiblex"),
    ("tenebrex", "Cyno Jammer"), ("cyno jammer", "Cyno Jammer"),
    ("pharolux", "Cyno Beacon"), ("cyno beacon", "Cyno Beacon"),
    ("pos", "POS"),
    ("poco", "POCO"),
    ("skyhook", "Skyhook"),
    ("metenox", "Metenox"), ("moon drill", "Metenox"),
    ("mercenary den", "Mercenary Den"), ("merc den", "Mercenary Den"),
    ("sovereignty hub", "Sov Hub"), ("sov hub", "Sov Hub"),
];

const STRUCTURE_TYPES: &[(&str, i64)] = &[
    ("Keepstar", 35834),
    ("Fortizar", 35833),
    ("Astrahus", 35832),
    ("Raitaru", 35825),
    ("Azbel", 35826),
    ("Sotiyo", 35827),
    ("Athanor", 35835),
    ("Tatara", 35836),
    ("Ansiblex", 35841),
    ("Cyno Jammer", 37534),
    ("Cyno Beacon", 35840),
    ("POCO", 2233),
    ("Metenox", 81826),
    ("Mercenary Den", 85230),
    ("Skyhook", 81080),
    ("Sov Hub", 81080),
];

pub fn structure_type_id(name: &str) -> Option<i64> {
    STRUCTURE_TYPES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, id)| *id)
}

pub fn structure_name_by_type(id: i64) -> Option<&'static str> {
    STRUCTURE_TYPES.iter().find(|(_, i)| *i == id).map(|(n, _)| *n)
}

fn is_structure_word(t: &str) -> bool {
    let lw = t.to_lowercase();
    STRUCTURES.iter().any(|(m, _)| !m.contains(' ') && *m == lw.as_str()) || is_skyhook_typo(&lw)
}

fn is_skyhook_typo(w: &str) -> bool {
    let w = w.to_lowercase();
    w.len() >= 5 && w.starts_with("sk") && crate::shipnames::edit_distance(&w, "skyhook") <= 1
}

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

fn belt_locations(words: &[String]) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for (i, w) in words.iter().enumerate() {
        if w != "belt" {
            continue;
        }
        let prev = i.checked_sub(1).and_then(|p| words.get(p)).map(String::as_str);
        let (start, len, label) = match prev {
            Some("ice") => (i - 1, 2, "Ice Belt"),
            Some("asteroid") => (i - 1, 2, "Asteroid Belt"),
            _ => (i, 1, "Belt"),
        };
        out.push((start, len, label.to_owned()));
    }
    out
}

fn roman_value(s: &str) -> i64 {
    let mut total = 0;
    let mut prev = 0;
    for c in s.chars().rev() {
        let v = match c.to_ascii_uppercase() {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            _ => 0,
        };
        if v < prev {
            total -= v;
        } else {
            total += v;
            prev = v;
        }
    }
    total
}

fn detect_celestials(tokens: &[&str]) -> (Vec<String>, Vec<String>) {
    let is_roman = |t: &str| {
        (1..=5).contains(&t.len())
            && !t.eq_ignore_ascii_case("i")
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
            if !n.is_empty()
                && n.starts_with(|c: char| c.is_ascii_digit())
                && n.chars().all(|c| c.is_ascii_digit() || c == '-')
            {
                let mut label = format!("{k} {n}");
                if k == "Moon" && !n.contains('-') {
                    let mut j = i;
                    while j > 0 {
                        j -= 1;
                        let t = tokens[j].trim_matches(|c: char| !c.is_ascii_alphanumeric());
                        if t.is_empty() {
                            continue;
                        }
                        if is_roman(t) {
                            label = format!("Moon {}-{n}", roman_value(t));
                            consumed.push(t.to_lowercase());
                        }
                        break;
                    }
                }
                push(label, &mut labels);
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

fn detect_probes(text: &str) -> Option<Probes> {
    let lower = text.to_lowercase();
    let core = lower.contains("core scanner") || lower.contains("core prob");
    let combat = lower.contains("combat scanner") || lower.contains("combat prob");
    match (core, combat) {
        (true, false) => Some(Probes::Core),
        (false, true) => Some(Probes::Combat),
        (true, true) => Some(Probes::Any),
        (false, false) => {
            let bare = lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| matches!(w, "probes" | "probs"));
            (lower.contains("scanner prob") || bare).then_some(Probes::Any)
        }
    }
}

fn structure_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.').to_lowercase())
        .collect()
}

fn structure_spans(words: &[String]) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
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
            if len == 1 && is_skyhook_typo(&phrase) {
                hit = Some((len, "Skyhook".to_string()));
                break;
            }
        }
        if let Some((len, canon)) = hit {
            out.push((i, len, canon));
            i += len;
        } else {
            i += 1;
        }
    }
    out
}

fn detect_structures(text: &str) -> Vec<(String, Option<String>)> {
    let words = structure_words(text);
    let dists: Vec<(usize, String)> = words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| parse_distance(w, words.get(i + 1).map(|s| s.as_str())).map(|d| (i, d)))
        .collect();
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for (i, _len, canon) in structure_spans(&words) {
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
    }
    out
}

/// returned in ISK. "kk" is the EVE shorthand for millions. Returns the largest match.
fn parse_isk(text: &str, ess: bool) -> Option<u64> {
    if !ess {
        return None;
    }
    let mult = |s: &str| -> Option<f64> {
        match s {
            "k" => Some(1e3),
            // "mio"/"mio." is an unambiguous "million" abbreviation (no system-code collision
            // like bare "m"), so it counts as 1e6 regardless of ESS context.
            "kk" | "mil" | "mill" | "million" | "millions" | "mio" | "mio." => Some(1e6),
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
            // In ESS context an amount below 50M is almost always a TIME, not ISK ("30m" = 30
            // minutes, not 30M ISK) — real ESS banks worth calling out are >= 50M. Drop the small
            // ones so they don't double-parse as an ISK amount alongside the hack timer.
            if ess && isk < 50_000_000 {
                continue;
            }
            if best.map_or(true, |b| isk > b) {
                best = Some(isk);
            }
        }
    }
    best
}

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

/// Derive the hostile count from its components and the CURRENT pilot count. `named` is the number
/// of pilots still in the report, so re-deriving after resolution drops a count that was inflated by
/// discarded candidates. An explicit total (`extra`) stands on its own; otherwise a `+N` addend or
/// 3+ named pilots or the solo keyword seeds the base. Resolved ship counts always add on top.
pub fn derive_count(
    extra: Option<u32>,
    plus: u32,
    ships: u32,
    named: u32,
    solo: bool,
) -> Option<u32> {
    let base = if let Some(t) = extra {
        t + plus
    } else if plus > 0 {
        named + plus
    } else if named >= 3 {
        named
    } else if solo {
        1
    } else {
        0
    };
    let total = (base + ships).min(999);
    (total > 0).then_some(total)
}

const COUNT_KEYWORDS: &[&str] =
    &["red", "reds", "neut", "neuts", "neutral", "neutrals", "hostile", "hostiles"];

fn is_plus_token(w: &str) -> bool {
    let t = w.trim();
    !t.is_empty() && t.chars().all(|c| c == '+')
}

fn is_count_keyword(w: &str) -> bool {
    let lw = w.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase();
    COUNT_KEYWORDS.contains(&lw.as_str())
}

fn is_ship_or_class_word(w: &str, ship_index: &HashMap<String, (i64, String)>) -> bool {
    let lw = w.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase();
    !lw.is_empty()
        && (SHIP_CLASSES.iter().any(|(k, _)| *k == lw.as_str()) || ship_of(&lw, ship_index).is_some())
}

fn parse_count(
    text: &str,
    consumed: &[String],
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
    pilots: &[String],
    known_pilots: &HashMap<String, i64>,
) -> (Option<u32>, u32, Vec<(String, u32)>) {
    let mut name_skips: Vec<(String, u32)> = Vec::new();
    const MAGNITUDE: &[&str] = &[
        "m", "mil", "mill", "million", "millions", "mio", "kk", "b", "bil", "bill", "billion",
        "billions", "k", "isk",
    ];
    let mut best: Option<u32> = None;
    let mut plus: u32 = 0;
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, raw) in words.iter().enumerate() {
        if raw.contains('-') {
            continue;
        }
        let t = raw
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '+' && c != 'x' && c != 'X')
            .to_ascii_lowercase();
        let t = t.as_str();
        let digits = t.trim_start_matches(['+', 'x']).trim_end_matches(['x', '+']);
        if digits.is_empty() || digits.len() > 3 {
            continue;
        }
        let attached_plus = t.starts_with('+') || t.ends_with('+');
        let attached_x = t.starts_with('x') || t.ends_with('x');
        let bare_number = t.chars().all(|c| c.is_ascii_digit());
        if !(attached_plus || attached_x || bare_number) {
            continue;
        }
        // A number is a hostile count only when qualified: a '+' (attached or a standalone
        // neighbour), an x/X multiplier, a red/neut/hostile keyword beside it, or a
        // ship/ship-class beside it. A lone number is too error-prone to count.
        let prev = i.checked_sub(1).map(|j| words[j]);
        let next = words.get(i + 1).copied();
        let plus_neighbour = prev.is_some_and(is_plus_token) || next.is_some_and(is_plus_token);
        let kw_neighbour = prev.is_some_and(is_count_keyword) || next.is_some_and(is_count_keyword);
        let ship_neighbour = prev.is_some_and(|w| is_ship_or_class_word(w, ship_index))
            || next.is_some_and(|w| is_ship_or_class_word(w, ship_index));
        // "N in system" / "N in local": the number is followed by "in" + a tight location word.
        // Keep the vocab tight so "5 in Rancer" (a system name) stays unqualified.
        let loc_neighbour = next.is_some_and(|w| w.eq_ignore_ascii_case("in"))
            && words.get(i + 2).is_some_and(|w| {
                let lw = w.trim_matches(|c: char| !c.is_alphanumeric()).to_ascii_lowercase();
                matches!(lw.as_str(), "system" | "systems" | "sys" | "local")
            });
        let qualified = attached_plus
            || attached_x
            || plus_neighbour
            || kw_neighbour
            || ship_neighbour
            || loc_neighbour;
        if bare_number
            && pilots.iter().any(|p| {
                let pl = p.to_lowercase();
                pl.split_whitespace().next() == Some(digits)
                    && known_pilots.keys().any(|k| {
                        k.contains(' ')
                            && k.split_whitespace().next() == Some(digits)
                            && pl.starts_with(k.as_str())
                    })
            })
        {
            continue;
        }
        if bare_number && !qualified && i > 0 {
            let prevw = words[i - 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-');
            let plc = prevw.to_lowercase();
            if name_part(prevw) && systems.lookup(prevw).is_none() && !ship_index.contains_key(&plc) {
                if let Ok(n) = digits.parse::<u32>() {
                    name_skips.push((format!("{prevw} {digits}"), n));
                }
                continue;
            }
        }
        if bare_number && !attached_plus && !attached_x {
            if consumed.iter().any(|c| c == &t.to_lowercase()) {
                continue;
            }
            if let Some(nx) = next {
                let n = nx.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                if MAGNITUDE.contains(&n.as_str()) {
                    continue;
                }
            }
        }
        if let Ok(n) = digits.parse::<u32>() {
            if (1..=999).contains(&n) {
                if attached_plus || plus_neighbour {
                    plus = (plus + n).min(999);
                } else if attached_x || kw_neighbour || ship_neighbour || loc_neighbour {
                    best = Some(best.map_or(n, |b| (b + n).min(999)));
                } else {
                    continue;
                }
            }
        }
    }
    (best, plus, name_skips)
}

pub(crate) fn tokenize(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\''))
        .map(|t| t.trim_matches('\''))
        .filter(|t| t.len() >= 2)
        .collect()
}

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

/// The max-ship-size class a wormhole passes, from a scout's words. "Extra large" (and xl/xlarge)
/// is XL; a bare "large"/"medium"/"small" is the hole class in a wormhole message.
fn parse_wh_size(lower: &str, lower_tokens: &[String]) -> Option<crate::wormholes::ShipSize> {
    use crate::wormholes::ShipSize;
    let has = |w: &str| lower_tokens.iter().any(|t| t == w);
    // XL variants must be tested before the "large" substring.
    if lower.contains("extra large") || lower.contains("extra-large") || lower.contains("xlarge") || has("xl") {
        Some(ShipSize::XLarge)
    } else if lower.contains("large") {
        Some(ShipSize::Large)
    } else if lower.contains("medium") || has("med") {
        Some(ShipSize::Medium)
    } else if lower.contains("frigate") || has("frig") || has("small") {
        Some(ShipSize::Frigate)
    } else {
        None
    }
}

fn looks_like_sig(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 7
        && b[3] == b'-'
        && b[..3].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit)
}

pub fn parse_eve_time(s: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y.%m.%d %H:%M:%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{SystemInfo, Systems};

    #[test]
    fn sightings_counting_and_revival() {
        let now = 1_000_000;
        let mut s = Sightings::default();
        s.record("Bob", 30000001, now - 100);
        s.record("bob", 30000002, now - 200);
        s.record("BOB", 30000003, now - 300);
        s.record("bob", 30000001, now - 50);
        s.record("bob", 30000099, now - 20000);

        assert_eq!(s.distinct_systems_since("bob", 3600, now), 3);
        assert!(s.revived("bob", now));

        assert_eq!(s.distinct_systems_since("bob", SIGHTINGS_WINDOW, now), 3);
        assert_eq!(s.distinct_systems_since("bob", 999_999, now), 4);
        s.prune(now);
        assert_eq!(s.distinct_systems_since("bob", 999_999, now), 3);

        assert_eq!(s.distinct_systems_since("nobody", 3600, now), 0);
        assert!(!s.revived("nobody", now));

        let mut w = Sightings::default();
        for (i, dt) in [(1, 100), (2, 200), (3, 5000), (4, 6000), (5, 7000)] {
            w.record("roamer", 30000000 + i, now - dt);
        }
        assert_eq!(w.distinct_systems_since("roamer", 3600, now), 2);
        assert_eq!(w.distinct_systems_since("roamer", SIGHTINGS_WINDOW, now), 5);
        assert!(w.revived("roamer", now));

        let mut z = Sightings::default();
        z.record("x", 0, now);
        z.record("x", -5, now);
        assert_eq!(z.distinct_systems_since("x", 3600, now), 0);
    }

    fn noships() -> std::collections::HashMap<String, (i64, String)> {
        std::collections::HashMap::new()
    }

    fn noknown() -> std::collections::HashMap<String, i64> {
        std::collections::HashMap::new()
    }

    fn esi_resolve(pilots: &[String], reals: &[&str]) -> Vec<String> {
        use crate::pilot::{name_windows, PilotCache};
        let real_map: std::collections::HashMap<String, i64> =
            reals.iter().enumerate().map(|(i, r)| (r.to_lowercase(), i as i64 + 1)).collect();
        let mut c = PilotCache::default();
        c.preload(&real_map);
        let mut negs: Vec<String> = Vec::new();
        for p in pilots {
            let mut spans = name_windows(p);
            spans.push(p.clone());
            spans.extend(p.split_whitespace().map(str::to_owned));
            for w in spans {
                let lw = w.to_lowercase();
                if !real_map.contains_key(&lw) {
                    negs.push(lw);
                }
            }
        }
        c.preload_negatives(&negs);
        let mut out: Vec<String> = Vec::new();
        for p in pilots {
            if is_pilot_stopword(p) {
                continue;
            }
            match c.get(p) {
                Some(Some(_)) => out.push(p.clone()),
                _ => out.extend(c.cover(p).into_iter().filter(|n| !is_pilot_stopword(n))),
            }
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|p| seen.insert(p.to_lowercase()));
        out
    }

    fn resolve_report(
        r: &IntelReport,
        reals: &[&str],
        systems: &Systems,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let pilots = esi_resolve(&r.pilots, reals);
        let reserved: std::collections::HashSet<String> =
            pilots.iter().flat_map(|p| p.split_whitespace()).map(|w| w.to_lowercase()).collect();
        let tokens: Vec<&str> = tokenize(&r.text);
        let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
        let (detected, gates, _) =
            detect_location(&tokens, &lower_tokens, &reserved, systems, None, &[]);
        (pilots, detected.into_iter().map(|d| d.name).collect(), gates)
    }

    fn apply_resolution(r: &mut IntelReport, reals: &[&str], systems: &Systems) {
        let (pilots, sysnames, gates) = resolve_report(r, reals, systems);
        r.pilots = pilots;
        r.systems = sysnames
            .iter()
            .filter_map(|n| resolve(systems, n))
            .map(|i| DetectedSystem { id: i.id, name: i.name.clone(), security: i.security })
            .collect();
        r.gates = gates;
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
            ("uitra", "Uitra", 30000148, 0.9),
            ("n3-jbx", "N3-JBX", 30000669, -0.3),
            ("384-in", "384-IN", 30000535, -0.5),
            ("e-jcus", "E-JCUS", 30000531, -0.5),
            ("b-3qpd", "B-3QPD", 30001156, -0.4),
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
    fn denied_name_frees_its_tokens() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("comet".to_string(), 1i64)].into_iter().collect();
        let empty = std::collections::HashSet::new();
        let base = analyze_ctx(
            "Comet tackled in Rancer", &s, &noships(), &known, 1, "ch", "x", None, &[], &empty,
        );
        assert!(
            base.pilots.iter().any(|p| p.eq_ignore_ascii_case("comet")),
            "baseline anchors Comet: {:?}",
            base.pilots
        );
        let denied: std::collections::HashSet<String> =
            ["comet".to_string()].into_iter().collect();
        let r = analyze_ctx(
            "Comet tackled in Rancer", &s, &noships(), &known, 1, "ch", "x", None, &[], &denied,
        );
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("comet")),
            "denied name Comet must be freed, not a pilot: {:?}",
            r.pilots
        );
        assert!(r.tackled, "the tackle keyword still parses with Comet freed");
        assert!(
            r.systems.iter().any(|d| d.name == "Rancer"),
            "the system still parses with Comet freed: {:?}",
            r.systems
        );
    }

    #[test]
    fn all_stop_word_runs_are_never_pilots() {
        assert!(is_pilot_stopword("they are"));
        assert!(is_pilot_stopword("back to"));
        assert!(is_pilot_stopword("still here"));
        assert!(is_pilot_stopword("they"));
        assert!(!is_pilot_stopword("bob"));
        assert!(!is_pilot_stopword("Navy Bob"));
        assert!(is_pilot_stopword("I'm"));
        assert!(is_pilot_stopword("im"));
        assert!(is_pilot_stopword("they're"));
        assert!(is_pilot_stopword("don't"));
        assert!(!is_pilot_stopword("O'Brien"));
        assert!(is_pilot_stopword("full"));
        assert!(is_pilot_stopword("Full"));
    }

    #[test]
    fn common_phrases_not_parsed_as_pilots() {
        let s = systems();
        let known = std::collections::HashMap::new();
        let empty = std::collections::HashSet::new();
        let a = |t: &str| analyze_ctx(t, &s, &noships(), &known, 1, "ch", "x", None, &[], &empty);

        for (text, banned) in [
            ("They are roaming in Rancer", &["they are", "they", "are"][..]),
            ("Back to gate in Rancer", &["back to", "back", "to"][..]),
            ("Still here in Rancer", &["still here", "still", "here"][..]),
        ] {
            let r = a(text);
            for b in banned {
                assert!(
                    !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(b)),
                    "{text:?}: {b:?} must not be a pilot: {:?}",
                    r.pilots
                );
            }
            assert!(
                r.systems.iter().any(|d| d.name == "Rancer"),
                "{text:?}: the system still parses with the prose freed: {:?}",
                r.systems
            );
        }

        let r = a("I'm tackled in Rancer");
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("i'm") || p.eq_ignore_ascii_case("im")),
            "I'm must not be a pilot: {:?}",
            r.pilots
        );
        assert!(r.tackled, "tackle keyword still parses with I'm freed");
        assert!(r.systems.iter().any(|d| d.name == "Rancer"));
    }

    #[test]
    fn legit_names_with_a_non_stop_word_survive() {
        let s = systems();
        let known = std::collections::HashMap::new();
        let empty = std::collections::HashSet::new();
        let a = |t: &str| analyze_ctx(t, &s, &noships(), &known, 1, "ch", "x", None, &[], &empty);

        let r = a("Bob Hope tackled in Rancer");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Bob Hope")),
            "multi-word name survives: {:?}",
            r.pilots
        );
        let r = a("I-Pustelga tackled in Rancer");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("I-Pustelga")),
            "distinctive single-word name survives: {:?}",
            r.pilots
        );

        let r = a("Navy Bob tackled in Rancer");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Navy Bob")),
            "one non-stop word keeps the run: {:?}",
            r.pilots
        );
    }

    fn systems_with_neighbor() -> Systems {
        let by_name = [("rancer", "Rancer", 1i64, 0.4), ("f2a-3x", "F2A-3X", 100, -0.4), ("jita", "Jita", 2, 0.9)]
            .into_iter()
            .map(|(k, n, id, sec)| {
                (k.to_string(), SystemInfo { id, name: n.to_string(), security: sec, constellation: String::new(), region: String::new(), faction: String::new() })
            })
            .collect();
        let adjacency = [(1i64, vec![100i64]), (100, vec![1])].into_iter().collect();
        Systems::new(by_name, adjacency)
    }

    #[test]
    fn short_code_resolves_as_neighbour_gate_not_pilot() {
        let s = systems_with_neighbor();
        for msg in ["Bob f2a", "hostiles F2A", "hostiles f2a-3"] {
            let r = analyze_ctx(msg, &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
            let on_f2a = r.gates.iter().any(|g| g.eq_ignore_ascii_case("F2A-3X"))
                || r.systems.iter().any(|d| d.name == "F2A-3X");
            assert!(on_f2a, "{msg}: F2A not resolved — gates={:?} systems={:?}", r.gates, r.systems.iter().map(|d| &d.name).collect::<Vec<_>>());
            assert!(!r.pilots.iter().any(|p| p.to_lowercase().contains("f2a")), "{msg}: F2A as pilot {:?}", r.pilots);
        }
        let r = analyze_ctx("Bob f2a", &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
        assert!(r.pilots.iter().any(|p| p == "Bob"), "Bob lost: {:?}", r.pilots);
    }

    #[test]
    fn surname_that_is_a_system_is_not_a_gate() {
        let s = systems();
        let r2 = analyze("N3-JBX* alexpanda Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
        let (pilots, sysd, gates) = resolve_report(&r2, &["alexpanda Uitra"], &s);
        assert_eq!(pilots, vec!["alexpanda Uitra".to_string()]);
        assert_eq!(sysd, vec!["N3-JBX".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
        let r3 = analyze("N3-JBX Bob Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
        let (pilots, sysd, gates) = resolve_report(&r3, &["Bob Uitra"], &s);
        assert_eq!(pilots, vec!["Bob Uitra".to_string()]);
        assert_eq!(sysd, vec!["N3-JBX".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
        let r4 = analyze("N3-JBX Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
        let (pilots, sysd, gates) = resolve_report(&r4, &[], &s);
        assert!(pilots.is_empty(), "pilots={pilots:?}");
        assert_eq!(sysd, vec!["N3-JBX".to_string()]);
        assert!(gates.iter().any(|g| g == "Uitra"), "gates={gates:?}");
    }

    #[test]
    fn confirmed_name_system_surname_not_pulled_as_gate() {
        let s = systems();
        for line in ["alexpanda Uitra", "alexpanda Uitra gate", "alexpanda Uitra tackled"] {
            let r = analyze(line, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(proposed(&r.pilots, "alexpanda Uitra"), "{line:?}: not proposed: {:?}", r.pilots);
            let (pilots, sysd, gates) = resolve_report(&r, &["alexpanda Uitra"], &s);
            assert_eq!(pilots, vec!["alexpanda Uitra".to_string()], "{line:?}: pilots={pilots:?}");
            assert!(sysd.is_empty(), "{line:?}: Uitra leaked as a system: {sysd:?}");
            assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "{line:?}: Uitra leaked as a gate: {gates:?}");
        }
        let r = analyze("Rancer alexpanda Uitra gate", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, sysd, gates) = resolve_report(&r, &["alexpanda Uitra"], &s);
        assert_eq!(pilots, vec!["alexpanda Uitra".to_string()], "pilots={pilots:?}");
        assert!(sysd.iter().any(|n| n == "Rancer"), "Rancer missing: {sysd:?}");
        assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "Uitra leaked as a gate: {gates:?}");
        let r = analyze("N3-JBX Uitra gate", &s, &noships(), &noknown(), 1, "ch", "x");
        let (_p, _sysd, gates) = resolve_report(&r, &[], &s);
        assert!(gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "genuine Uitra gate lost: {gates:?}");
    }

    fn proposed(pilots: &[String], name: &str) -> bool {
        let want: Vec<String> = name.split_whitespace().map(|w| w.to_lowercase()).collect();
        pilots.iter().any(|p| {
            let ws: Vec<String> = p.split_whitespace().map(|w| w.to_lowercase()).collect();
            ws.windows(want.len()).any(|w| w == want.as_slice())
        })
    }

    fn has_pilot_token(pilots: &[String], tok: &str) -> bool {
        pilots.iter().any(|p| p.split_whitespace().any(|w| w.eq_ignore_ascii_case(tok)))
    }

    #[test]
    fn stray_letter_midrun_splits_pilot_list() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("willlin".to_string(), 1i64), ("qiuxiaoye".to_string(), 2i64)].into_iter().collect();
        let r = analyze(
            "willlin qiuxiaoye Micahel wu v Htguuu Htg-0 灵感级* 金鹏级*",
            &s,
            &noships(),
            &known,
            1,
            "ch",
            "Wujian",
        );
        for name in ["willlin", "qiuxiaoye", "Micahel wu", "Htguuu", "Htg-0"] {
            assert!(proposed(&r.pilots, name), "{name:?} not proposed: {:?}", r.pilots);
        }
        assert!(!has_pilot_token(&r.pilots, "v"), "stray v leaked: {:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p == "v"));
        assert!(proposed(&r.pilots, "Htg-0"), "Htg-0 mangled: {:?}", r.pilots);
        assert!(!has_pilot_token(&r.pilots, "灵感级"), "ship as pilot: {:?}", r.pilots);
    }

    #[test]
    fn stray_word_midrun_splits_pilot_list() {
        let s = systems();
        let r = analyze("Alpha v Bravo", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Alpha"), "Alpha lost: {:?}", r.pilots);
        assert!(r.pilots.iter().any(|p| p == "Bravo"), "Bravo lost: {:?}", r.pilots);
        assert!(!has_pilot_token(&r.pilots, "v"), "stray v leaked: {:?}", r.pilots);

        let r2 = analyze("Alpha Bravo lol Charlie", &s, &noships(), &noknown(), 1, "ch", "x");
        for name in ["Alpha", "Bravo", "Charlie"] {
            assert!(proposed(&r2.pilots, name), "{name:?} not proposed: {:?}", r2.pilots);
        }
        assert!(!has_pilot_token(&r2.pilots, "lol"), "stray lol leaked: {:?}", r2.pilots);

        let r3 = loose_pilot_runs("Cult is Dead", &noships(), &s);
        assert!(r3.iter().any(|p| p == "Cult is Dead"), "Cult is Dead split: {r3:?}");
    }

    #[test]
    fn stray_letter_before_name_with_code_system() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("ruston shackleford".to_string(), 95786689i64)].into_iter().collect();
        let rk =
            analyze("v Ruston Shackleford B-3QPD", &s, &noships(), &known, 1, "ch", "Ixen Orlenard");
        assert_eq!(rk.pilots, vec!["Ruston Shackleford".to_string()], "known pilots={:?}", rk.pilots);
        assert_eq!(
            rk.systems.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
            vec!["B-3QPD".to_string()],
            "known systems"
        );
        assert!(rk.gates.is_empty(), "gates={:?}", rk.gates);

        let r = analyze("v Ruston Shackleford B-3QPD", &s, &noships(), &noknown(), 1, "ch", "Ixen Orlenard");
        let (pilots, sysd, gates) = resolve_report(&r, &["Ruston Shackleford"], &s);
        assert_eq!(pilots, vec!["Ruston Shackleford".to_string()], "raw pilots={:?}", r.pilots);
        assert_eq!(sysd, vec!["B-3QPD".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
    }

    #[test]
    fn held_model_lowercase_name_with_system() {
        let s = systems();
        let r = analyze("C-J6MT bob uitra", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.systems.is_empty(), "location must be held: {:?}", r.systems);
        assert!(has_held_system(&r, &s), "report should be parked");
        let (pilots, sysd, gates) = resolve_report(&r, &["bob uitra"], &s);
        assert_eq!(pilots, vec!["bob uitra".to_string()]);
        assert_eq!(sysd, vec!["C-J6MT".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
        let (pilots, sysd, _) = resolve_report(&r, &[], &s);
        assert!(pilots.is_empty(), "pilots={pilots:?}");
        assert!(sysd.iter().any(|n| n == "C-J6MT"), "systems={sysd:?}");
    }

    #[test]
    fn fly_catcher_is_the_flycatcher_hull() {
        let s = systems();
        let mut by_name = std::collections::HashMap::new();
        by_name.insert("flycatcher".to_string(), (16242i64, "Flycatcher".to_string()));
        let mut ships = by_name.clone();
        for (slug, e) in crate::shipnames::aliases(&by_name) {
            ships.entry(slug).or_insert(e);
        }
        let r = analyze("Fly Catcher on gate in Jita", &s, &ships, &noknown(), 1, "ch", "Scout");
        assert!(r.ships.iter().any(|sh| sh.name == "Flycatcher"), "ships={:?}", r.ships);
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("fly") || p.eq_ignore_ascii_case("catcher")),
            "pilots={:?}",
            r.pilots
        );
    }

    #[test]
    fn system_gate_token_is_not_also_a_pilot() {
        let mut by_name = std::collections::HashMap::new();
        for (key, name, id, sec) in
            [("o3-4mn", "O3-4MN", 100i64, -0.5f64), ("ias-x", "IAS-X", 101, -0.5)]
        {
            by_name.insert(
                key.to_string(),
                SystemInfo {
                    id,
                    name: name.to_string(),
                    security: sec,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            );
        }
        let adjacency = [(100i64, vec![101i64]), (101, vec![100])].into_iter().collect();
        let s = Systems::new(by_name, adjacency);
        let r = analyze("O3-4MN gang on the IAS gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
        assert!(r.gates.iter().any(|g| g == "IAS-X"), "gates={:?}", r.gates);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("ias")), "pilots={:?}", r.pilots);
        assert_eq!(
            r.systems.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
            vec!["O3-4MN"],
            "systems={:?}",
            r.systems
        );
    }

    #[test]
    fn all_caps_names_are_pilots_regardless_of_length() {
        let s = systems();
        let r = analyze("C-J6MT  PORTOS11", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(esi_resolve(&r.pilots, &["PORTOS11"]), vec!["PORTOS11".to_string()]);
        let r2 = analyze("XEN in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(esi_resolve(&r2.pilots, &["XEN"]), vec!["XEN".to_string()]);
    }

    #[test]
    fn safe_is_a_clear_and_question_mark_suppresses_it() {
        let s = systems();
        assert!(analyze("Rancer safe", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(!analyze("Rancer clear?", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(!analyze("is Rancer safe?", &s, &noships(), &noknown(), 1, "ch", "x").clear);
    }

    #[test]
    fn paste_segment_is_not_unglued_by_the_cache() {
        let s = systems();
        let mut known = noknown();
        known.insert("ghost".into(), 1);
        known.insert("magician".into(), 2);
        let paste = analyze("C-J6MT  Ghost Magician", &s, &noships(), &known, 1, "ch", "x");
        assert_eq!(paste.pilots, vec!["Ghost Magician".to_string()], "{:?}", paste.pilots);
        let typed = analyze("Ghost Magician in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert_eq!(typed.pilots, vec!["Ghost Magician".to_string()], "{:?}", typed.pilots);
        let mut k3 = known.clone();
        k3.insert("gliar".into(), 3);
        k3.insert("mliarvis".into(), 4);
        k3.insert("sliarhia".into(), 5);
        let list = analyze("Gliar Mliarvis Sliarhia in Rancer", &s, &noships(), &k3, 1, "ch", "x");
        assert_eq!(list.pilots.len(), 1, "kept whole at parse time: {:?}", list.pilots);
        let split = esi_resolve(&list.pilots, &["Gliar", "Mliarvis", "Sliarhia"]);
        assert_eq!(split.len(), 3, "ESI-rejected whole + confirmed handles → list: {:?}", split);
        assert!(esi_resolve(&paste.pilots, &["Ghost", "Magician"]).is_empty());
    }

    #[test]
    fn paste_segment_drops_typed_location_tail() {
        let s = systems();
        let r = analyze("C-J6MT  Garen Willow at taj", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["Garen Willow".to_string()], "pilots={:?}", r.pilots);
        assert!(r.systems.iter().any(|d| d.name == "C-J6MT"));
        assert_eq!(trim_paste_location_tail("Man in Black", &ships_with(&[])), "Man in Black");
        assert_eq!(trim_paste_location_tail("Lord of War", &ships_with(&[])), "Lord of War");
        assert_eq!(trim_paste_location_tail("Garen Willow at taj", &ships_with(&[])), "Garen Willow");
    }

    #[test]
    fn paste_segment_drops_trailing_count() {
        let s = systems();
        let r = analyze("C-J6MT  01XcerberusX01 +3", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["01XcerberusX01".to_string()], "pilots={:?}", r.pilots);
        assert_eq!(r.count, Some(4), "count={:?}", r.count);
        assert_eq!(trim_paste_location_tail("Malcolm 41", &ships_with(&[])), "Malcolm 41");
        assert_eq!(trim_paste_location_tail("01XcerberusX01 +3", &ships_with(&[])), "01XcerberusX01");
        assert_eq!(trim_paste_location_tail("Drake x4", &ships_with(&[])), "Drake");
    }

    #[test]
    fn pasted_urls_are_not_parsed_as_pilots() {
        let s = systems();
        let r = analyze("https://dscan.info/v/a626d009ffc3  Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.is_empty(), "url leaked as pilots: {:?}", r.pilots);
        assert_eq!(r.links.len(), 1, "dscan link should be captured");
        assert!(r.systems.iter().any(|d| d.name == "Rancer"));
        let r2 = analyze("Bob https://example.com/Foo-Bar in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r2.pilots.iter().any(|p| p.to_lowercase().contains("foo") || p.contains("example") || p.contains("http")), "url fragments leaked: {:?}", r2.pilots);
        assert!(r2.pilots.iter().any(|p| p == "Bob"), "real name dropped: {:?}", r2.pilots);
    }

    #[test]
    fn belt_is_a_location_badge_not_a_pilot() {
        let s = systems();
        let r = analyze("Ice Belt in Jita", &s, &noships(), &noknown(), 1, "ch", "Scout");
        assert!(r.celestials.iter().any(|c| c == "Ice Belt"), "celestials={:?}", r.celestials);
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);

        let r2 =
            analyze("hostiles at Asteroid Belt in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            r2.celestials.iter().any(|c| c == "Asteroid Belt"),
            "celestials={:?}",
            r2.celestials
        );
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("belt")), "pilots={:?}", r2.pilots);

        let r3 = analyze("camp on belt in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.celestials.iter().any(|c| c == "Belt"), "celestials={:?}", r3.celestials);
    }

    #[test]
    fn lowercase_full_name_not_truncated_to_surname() {
        let s = systems();
        let mut known2 = noknown();
        known2.insert("ji wuming".into(), 2112339969);
        known2.insert("wuming".into(), 999);
        let r2 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known2, 1, "ch", "x");
        assert_eq!(r2.pilots, vec!["ji wuming".to_string()], "got {:?}", r2.pilots);
        let mut known3 = noknown();
        known3.insert("wuming".into(), 999);
        let r3 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known3, 1, "ch", "x");
        assert_eq!(r3.pilots, vec!["ji wuming".to_string()], "got {:?}", r3.pilots);
    }

    #[test]
    fn full_name_not_split_into_ship_and_pilot() {
        let s = systems();
        let mut ships = noships();
        ships.insert("wolf".into(), (11371, "Wolf".into()));
        let mut known = noknown();
        known.insert("wolf e kristjansson".into(), 2122822665);
        let r2 = analyze("Wolf E Kristjansson nv", &s, &ships, &known, 1, "ch", "x");
        assert_eq!(r2.pilots, vec!["Wolf E Kristjansson".to_string()]);
        assert!(r2.ships.is_empty(), "ships={:?}", r2.ships);
    }

    #[test]
    fn rest_keyword_not_a_pilot_even_if_known() {
        let s = systems();
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
        let mut known = noknown();
        for (w, id) in [("sry", 1i64), ("gg", 2), ("ez", 3), ("neo", 4)] {
            known.insert(w.into(), id);
        }
        let r = analyze("sry gg ez that was ez in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
        let r2 = analyze("Neo tackled in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
        assert!(r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("neo")), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn name_with_bubble_keyword_is_a_pilot_not_a_bubble() {
        let mut by_name = std::collections::HashMap::new();
        by_name.insert("r0-dmm".to_string(), SystemInfo { id: 30000563, name: "R0-DMM".into(),
            security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
        let s = Systems::new(by_name, HashMap::new());
        let r = analyze("R0-DMM  The Bubble Boy", &s, &noships(), &noknown(), 1, "ch", "Anniken");
        assert_eq!(r.pilots, vec!["The Bubble Boy".to_string()]);
        assert!(!r.bubble);
        assert!(analyze("bubble up on gate R0-DMM", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
        assert!(!analyze("2 Dragoons on gate R0-DMM", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
        assert!(analyze("drag bubble on the R0-DMM gate", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
    }

    #[test]
    fn standing_color_led_name_reaches_the_cover() {
        let mut by_name = std::collections::HashMap::new();
        by_name.insert("9olq-6".to_string(), SystemInfo { id: 30000800, name: "9OLQ-6".into(),
            security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
        let s = Systems::new(by_name, HashMap::new());
        let r = analyze("Blue RandomAttac Redhorn Mastro 9OLQ-6", &s, &noships(), &noknown(), 1, "ch", "Ariel Afuran");
        let (pilots, sysd, _) = resolve_report(&r, &["Blue RandomAttac", "Redhorn Mastro"], &s);
        assert_eq!(pilots, vec!["Blue RandomAttac".to_string(), "Redhorn Mastro".to_string()]);
        assert!(sysd.iter().any(|d| d == "9OLQ-6"), "systems={sysd:?}");
    }

    #[test]
    fn suffix_subphrase_pilot_is_dropped() {
        let s = systems();
        let r = analyze("Dr Chen Chen in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["Dr Chen Chen".to_string()]);
    }

    #[test]
    fn isk_amount_is_not_a_count() {
        let s = systems();
        let ships = ships_with(&[("Bellicose", 632)]);
        let r = analyze("ESS raid 2 Bellicose 334 million 6:00 Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(r.count, Some(2), "ISK amount must not inflate the count");
        let r2 = analyze("ESS raid 2 Bellicose 300 kk 6:00 Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(r2.count, Some(2), "300 kk must not be counted: {:?}", r2.count);
        let r3 = analyze("ESS raid 2 Bellicose 5 bill 6:00 Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(r3.count, Some(2), "5 bill must not be counted: {:?}", r3.count);
    }

    #[test]
    fn adjacent_names_not_leaked_as_subword() {
        let s = systems();
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
        let glued = "EVE System > Channel MOTD: TENERIFIS // IMMENSEA // IMPASS // CATCHPlease contact Corps";
        assert_eq!(parse_motd_regions(glued, &known), vec!["tenerifis", "immensea", "impass", "catch"]);
        assert_eq!(parse_motd_regions("Channel MOTD:  Wicked Creek //  Cache", &known), vec!["wicked creek"]);
        let utf8 = "Channel MOTD: Привет диплома // CATCH glued";
        assert_eq!(parse_motd_regions(utf8, &known), vec!["catch"]);
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
        let r0 = analyze_ctx("hostiles in C-J", &sys, &noships(), &noknown(), 1, "ch", "x", None, &[], &std::collections::HashSet::new());
        assert!(r0.systems.is_empty(), "should stay ambiguous: {:?}", r0.systems);
        let regions = vec!["Tenerifis".to_string()];
        let r = analyze_ctx(
            "hostiles in C-J", &sys, &noships(), &noknown(), 1, "ch", "x", None, &regions,
            &std::collections::HashSet::new(),
        );
        assert!(r.systems.iter().any(|s| s.name == "C-J6MT"), "systems={:?}", r.systems);
        assert!(!r.systems.iter().any(|s| s.name == "C-J7CR"), "systems={:?}", r.systems);
    }

    #[test]
    fn digit_handle_is_a_pilot_candidate() {
        let s = systems();
        let r = analyze("0xtomorrow AGCP-I", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["0xtomorrow"], &s);
        assert_eq!(pilots, vec!["0xtomorrow".to_string()], "pilots={pilots:?}");
        let junk = analyze("334m 88A 1DH-SX in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(esi_resolve(&junk.pilots, &[]).is_empty(), "junk pilots: {:?}", junk.pilots);
        assert!(is_time_token("4min") && is_time_token("30s") && is_time_token("2h"));
        assert!(!is_time_token("0xtomorrow") && !is_time_token("c137m"));
    }

    #[test]
    fn trailing_apostrophe_stripped_from_name() {
        let s = systems();
        let r = analyze("MO-I1W PeshyHod'", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["PeshyHod"], &s);
        assert_eq!(pilots, vec!["PeshyHod".to_string()], "pilots={pilots:?}");
        let r2 = analyze("O'Brien in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r2, &["O'Brien"], &s);
        assert_eq!(pilots, vec!["O'Brien".to_string()], "pilots={pilots:?}");
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
    fn currently_is_never_a_pilot() {
        let s = systems();
        let r = analyze("currently in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(is_pilot_stopword("currently"));
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("currently")),
            "currently parsed as pilot: {:?}",
            r.pilots
        );
        let r2 = analyze("Currently camped", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(esi_resolve(&r2.pilots, &[]).is_empty(), "pilots: {:?}", r2.pilots);
    }

    #[test]
    fn anom_sig_keyword_alone_raises_a_bare_badge() {
        let s = systems();
        let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
        for (kw, kind) in [
            ("anom", AnomKind::Anomaly),
            ("sig", AnomKind::Signature),
            ("anomaly", AnomKind::Anomaly),
            ("signature", AnomKind::Signature),
        ] {
            let r = a(kw);
            assert_eq!(r.anom_sigs, vec![(kind, String::new())], "{kw}: anom_sigs={:?}", r.anom_sigs);
            assert!(!r.diamond_rats, "{kw}: diamond_rats");
            assert!(esi_resolve(&r.pilots, &[]).is_empty(), "{kw}: pilots={:?}", r.pilots);
            assert!(r.systems.is_empty(), "{kw}: systems={:?}", r.systems);
        }
    }

    #[test]
    fn diamond_rats_badge_not_a_pilot() {
        let s = systems();
        for txt in ["diamond rats in Rancer", "dia rats", "Diamond Rats", "diamond rat"] {
            let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(r.diamond_rats, "{txt}: diamond_rats not set");
            let pilots = esi_resolve(&r.pilots, &["Diamond", "Dia", "Rat", "Rats"]);
            assert!(
                !pilots.iter().any(|p| {
                    matches!(p.to_lowercase().as_str(), "diamond" | "dia" | "rat" | "rats")
                }),
                "{txt}: rats word as pilot: {pilots:?}"
            );
        }
        let plain = analyze("rats on gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!plain.diamond_rats, "plain rats set diamond flag");
        assert!(esi_resolve(&plain.pilots, &["Rats"]).is_empty(), "plain pilots: {:?}", plain.pilots);
    }

    #[test]
    fn anom_sig_code_badge_both_orders() {
        let s = systems();
        let before = analyze("anom ABC-123", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(before.anom_sigs, vec![(AnomKind::Anomaly, "ABC-123".to_string())]);
        let after = analyze("ABC-123 sig", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(after.anom_sigs, vec![(AnomKind::Signature, "ABC-123".to_string())]);
        for r in [&before, &after] {
            assert!(esi_resolve(&r.pilots, &[]).is_empty(), "pilots: {:?}", r.pilots);
            assert!(r.systems.is_empty(), "systems: {:?}", r.systems);
        }
        assert_eq!(alert_label(&before.anom_sigs[0]), "Anom ABC-123");
        assert_eq!(alert_label(&after.anom_sigs[0]), "Sig ABC-123");
    }

    fn alert_label((kind, code): &(AnomKind, String)) -> String {
        match kind {
            AnomKind::Anomaly => format!("Anom {code}"),
            AnomKind::Signature => format!("Sig {code}"),
        }
    }

    #[test]
    fn distance_token_not_a_pilot_and_anomaly_badge() {
        let by_name = [
            ("27-hp0", "27-HP0", 30000832i64, -0.4),
            ("mordunium", "Mordunium", 30000833, -0.4),
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
        let s = Systems::new(by_name, HashMap::new());
        let ships = ships_with(&[("Vagabond", 11999)]);
        let known: std::collections::HashMap<String, i64> =
            [("tinde erkkinen".to_string(), 1i64)].into_iter().collect();
        let r = analyze(
            "27-HP0  tinde Erkkinen (Vagabond) 100km off Mordunium anomaly",
            &s,
            &ships,
            &known,
            1,
            "ch",
            "Lancer Maelstorm",
        );
        assert!(r.pilots.iter().any(|p| p == "tinde Erkkinen"), "pilots={:?}", r.pilots);
        assert!(
            !r.pilots.iter().any(|p| p.to_lowercase().contains("km")),
            "distance leaked into a pilot: {:?}",
            r.pilots
        );
        assert!(
            r.anom_sigs.iter().any(|(k, _)| *k == AnomKind::Anomaly),
            "no anomaly badge: {:?}",
            r.anom_sigs
        );
        assert!(r.ships.iter().any(|sh| sh.name == "Vagabond"), "ships={:?}", r.ships);
    }

    #[test]
    fn anom_sig_code_shape_letters_optional_digits() {
        let s = systems();
        let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            a("anomaly ABX").anom_sigs.iter().any(|(k, c)| *k == AnomKind::Anomaly && c == "ABX"),
            "ABX: {:?}",
            a("anomaly ABX").anom_sigs
        );
        assert!(
            a("ABC-123 sig").anom_sigs.iter().any(|(k, c)| *k == AnomKind::Signature && c == "ABC-123"),
            "ABC-123: {:?}",
            a("ABC-123 sig").anom_sigs
        );
        assert_eq!(a("the anomaly").anom_sigs, vec![(AnomKind::Anomaly, String::new())]);
    }

    #[test]
    fn anom_code_that_is_a_real_system_stays_a_system() {
        let by_name = [("abc-123", "ABC-123", 50001, -0.5)]
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
        let s = Systems::new(by_name, HashMap::new());
        let r = analyze("anom ABC-123", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            !r.anom_sigs.iter().any(|(_, c)| !c.is_empty()),
            "real system made an anom code: {:?}",
            r.anom_sigs
        );
        assert!(
            r.systems.iter().any(|d| d.name == "ABC-123"),
            "real system not detected: {:?}",
            r.systems
        );
    }

    #[test]
    fn chinese_hull_name_resolves_as_ship() {
        let s = systems();
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
        let r2 = analyze("2 marauders pointed", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.tackled && r2.tackled_targets.iter().any(|t| t == "Marauder"), "targets={:?}", r2.tackled_targets);
        let r3 = analyze("recon scrammed", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.tackled && r3.tackled_targets.iter().any(|t| t == "Recon"), "targets={:?}", r3.tackled_targets);
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
        let r3 = analyze("3 t3s and a t3 roaming, etc", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.classes.iter().any(|c| c == "Strategic Cruiser"), "classes={:?}", r3.classes);
        assert!(!r3.pilots.iter().any(|p| p.eq_ignore_ascii_case("etc")), "pilots={:?}", r3.pilots);
        let r4 = analyze("CRUISERS and battleships in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        // Generic hull tiers are just a size, not a class badge (only T2/T3 + capitals are).
        assert!(!r4.classes.iter().any(|c| c == "Cruiser"), "classes={:?}", r4.classes);
        assert!(!r4.classes.iter().any(|c| c == "Battleship"), "classes={:?}", r4.classes);
        assert!(esi_resolve(&r4.pilots, &[]).is_empty(), "pilots={:?}", r4.pilots);
        let mut ships = noships();
        ships.insert("dni".into(), (37457, "Drake Navy Issue".into()));
        let r5 = analyze("DNI in Jita", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r5.ships.iter().any(|sh| sh.name == "Drake Navy Issue"), "ships={:?}", r5.ships);
        assert!(r5.pilots.is_empty(), "pilots={:?}", r5.pilots);
        assert!(r2.classes.iter().any(|c| c == "Logistics"), "classes={:?}", r2.classes);
        assert!(r2.classes.iter().any(|c| c == "Stealth Bomber"), "classes={:?}", r2.classes);
    }

    #[test]
    fn class_word_in_pilot_name_is_not_a_class() {
        let s = systems();
        // A generic hull word in a pilot's name never becomes a class badge.
        let r = analyze("Bob Destroyer tackled in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r.classes.iter().any(|c| c == "Destroyer"), "classes={:?} pilots={:?}", r.classes, r.pilots);
        // A bare hull tier is never a class badge.
        let r2 = analyze("battleship gang in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.classes.is_empty(), "classes={:?}", r2.classes);
        // A standalone specialised class word still detects the class.
        let r3 = analyze("2 dictors on gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.classes.iter().any(|c| c == "Interdictor"), "classes={:?}", r3.classes);
    }

    #[test]
    fn alliance_name_not_double_consumed_as_pilots() {
        let s = systems();
        let r = analyze("Shadow Cartel gang in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.alliances.iter().any(|(n, _)| n == "Shadow Cartel"), "alliances={:?}", r.alliances);
        for w in ["Shadow", "Cartel", "Shadow Cartel"] {
            assert!(
                !r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)),
                "{w:?} leaked as a pilot: {:?}",
                r.pilots
            );
        }
    }

    #[test]
    fn ceno_resolves_to_cenotaph() {
        let s = systems();
        // Aliases are folded into the ship index (store.rs does this from the SDE); mimic that.
        let mut by: std::collections::HashMap<String, (i64, String)> = std::collections::HashMap::new();
        by.insert("cenotaph".into(), (85062i64, "Cenotaph".into()));
        for (slug, e) in crate::shipnames::aliases(&by) {
            by.insert(slug, e);
        }
        for msg in ["2 ceno on gate in Rancer", "cenos in Rancer"] {
            let r = analyze(msg, &s, &by, &noknown(), 1, "ch", "x");
            assert!(r.ships.iter().any(|sh| sh.name == "Cenotaph"), "{msg:?}: ships={:?}", r.ships);
        }
    }

    #[test]
    fn cleared_and_shiptype_ignored() {
        let s = systems();
        // "cleared" registers as a clear and is not a pilot.
        let r = analyze("Rancer cleared", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.clear, "cleared should register as clear: {:?}", r.text);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("cleared")), "pilots={:?}", r.pilots);
        // "ship type" / "shiptypes" is a common question, never a pilot.
        for msg in ["ship type?", "what shiptypes?"] {
            let r = analyze(msg, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(
                !r.pilots.iter().any(|p| {
                    let l = p.to_lowercase();
                    l.contains("type") || l.contains("ship")
                }),
                "{msg:?}: pilots={:?}",
                r.pilots
            );
        }
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
        let resolved = esi_resolve(&r.pilots, &[]);
        assert!(resolved.is_empty(), "ships/keywords resolved as pilots: {resolved:?}");
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
        let r = analyze("ZD1-Z2 Sabre Orthrus in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        for w in ["Sabre", "Orthrus", "Sabre Orthrus"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
        }
        assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "ships={:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Orthrus"), "ships={:?}", r.ships);
        let r2 = analyze("Stabber and Deimos in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r2.ships.iter().any(|sh| sh.name == "Stabber"), "ships={:?}", r2.ships);
        assert!(r2.ships.iter().any(|sh| sh.name == "Deimos"), "ships={:?}", r2.ships);
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Stabber") || p.eq_ignore_ascii_case("Deimos")), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn fuzzy_typo_multiword_hull_is_a_ship_not_pilots() {
        let s = systems();
        let ships = ships_with(&[
            ("Scythe Fleet Issue", 17812),
            ("Scythe", 631),
            ("Cyclone Fleet Issue", 17634),
            ("Drake", 24698),
        ]);
        let r = analyze("cythe fleet issue tackled in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r.ships);
        for w in ["cythe", "fleet issue", "fleet", "issue", "scythe", "cythe fleet issue"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
        }
        assert!(!r.ships.iter().any(|sh| sh.name == "Scythe"), "ships={:?}", r.ships);
        assert!(r.systems.iter().any(|d| d.name == "Rancer"), "systems={:?}", r.systems);
        assert!(r.tackled, "tackled keyword should fire");

        let r2 = analyze("Scythe Fleet Issue in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r2.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r2.ships);

        let r3 = analyze("draek in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r3.ships.iter().any(|sh| sh.name == "Drake"), "ships={:?}", r3.ships);
        let r4 = analyze("drak in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(!r4.ships.iter().any(|sh| sh.name == "Drake"), "ships={:?}", r4.ships);
    }

    #[test]
    fn confirmed_pilot_near_a_hull_stays_a_pilot() {
        let s = systems();
        let ships = ships_with(&[("Cyclone Fleet Issue", 17634)]);
        let known: std::collections::HashMap<String, i64> =
            [("cyclon fleet issue".to_string(), 4242i64)].into_iter().collect();
        let r = analyze("Cyclon Fleet Issue in Rancer", &s, &ships, &known, 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Cyclon Fleet Issue")),
            "pilots={:?}",
            r.pilots
        );
        assert!(!r.ships.iter().any(|sh| sh.name == "Cyclone Fleet Issue"), "ships={:?}", r.ships);
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
        sys.add_bridges(&[(1, 2)]);
        let r = analyze("O3-4MN Gate camp on Ansi", &sys, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.camp, "gate-camp keyword should fire");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ansi")), "pilots={:?}", r.pilots);
        assert!(r.gates.iter().any(|g| g == "Rancer"), "the Ansi should lead to Rancer: {:?}", r.gates);
    }

    #[test]
    fn system_code_known_as_pilot_is_not_a_pilot() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("c-j".to_string(), 2119528359i64)].into_iter().collect();
        let r = analyze("Gorika Galrog C-J in Rancer", &s, &noships(), &known, 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["Gorika Galrog"], &s);
        assert_eq!(pilots, vec!["Gorika Galrog".to_string()], "pilots={pilots:?}");
    }

    #[test]
    fn plus_count_adds_to_named_pilots() {
        let s = systems();
        let r = analyze("Gorika Galrog +20 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Gorika Galrog"), "pilots={:?}", r.pilots);
        assert_eq!(r.count, Some(21), "pilots={:?}", r.pilots);
        let r2 = analyze("Gorika Galrog in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.count, None, "pilots={:?}", r2.pilots);
    }

    #[test]
    fn derive_count_tracks_surviving_pilots() {
        // 3+ named pilots -> that many; a drop below 3 shows no bare count (parse semantics).
        assert_eq!(derive_count(None, 0, 0, 4, false), Some(4));
        assert_eq!(derive_count(None, 0, 0, 3, false), Some(3));
        assert_eq!(derive_count(None, 0, 0, 2, false), None);
        // A +N addend survives when its named pilots are discarded.
        assert_eq!(derive_count(None, 20, 0, 1, false), Some(21));
        assert_eq!(derive_count(None, 20, 0, 0, false), Some(20));
        // An explicit total (x5 / ship count) stands on its own, ignoring named pilots.
        assert_eq!(derive_count(Some(5), 0, 0, 2, false), Some(5));
        // Resolved ship counts add on top; solo seeds 1; nothing -> None.
        assert_eq!(derive_count(None, 0, 3, 0, false), Some(3));
        assert_eq!(derive_count(None, 0, 0, 0, true), Some(1));
        assert_eq!(derive_count(None, 0, 0, 0, false), None);
    }

    #[test]
    fn discarded_pilot_stops_inflating_count() {
        let s = systems();
        let r = analyze("Gorika Galrog +20 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.count, Some(21));
        assert_eq!(r.count_plus, 20);
        assert_eq!(r.count_extra, None);
        // If "Gorika Galrog" is later discarded (ESI: not a character), re-deriving from the
        // surviving pilots gives 0 named + 20 = 20, not the stale 21.
        let after = derive_count(r.count_extra, r.count_plus, r.count_ships, 0, r.solo);
        assert_eq!(after, Some(20));
    }

    #[test]
    fn combat_prob_is_probes_not_pilots() {
        let s = systems();
        let r = analyze("combat prob in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.probes, Some(Probes::Combat), "probes={:?}", r.probes);
        assert!(
            !r.pilots.iter().any(|p| {
                p.eq_ignore_ascii_case("combat") || p.eq_ignore_ascii_case("prob")
            }),
            "pilots={:?}",
            r.pilots
        );
    }

    #[test]
    fn thera_hole_is_a_wormhole() {
        let s = systems();
        let r = analyze("thera hole in Rancer", &s, &noships(), &noknown(), 1, "ch", "wwhh");
        assert!(r.wormhole, "should be a wormhole message");
        assert!(matches!(r.wh_dest, Some(crate::wormholes::DestClass::Thera)), "dest={:?}", r.wh_dest);
    }

    #[test]
    fn nullified_is_a_flag_not_a_pilot() {
        let s = systems();
        let ships = ships_with(&[("Loki", 29990)]);
        let r = analyze("Nullified Loki on gate in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.nullified, "nullified flag should fire");
        assert!(
            !proposed(&r.pilots, "Nullified") && !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("nullified")),
            "nullified leaked as a pilot: {:?}",
            r.pilots
        );
        // "nullifier" (the module name) also triggers it.
        assert!(analyze("ceptor with interdiction nullifier", &s, &noships(), &noknown(), 1, "ch", "x").nullified);
        // A plain nullsec mention must NOT trigger it.
        assert!(!analyze("hostiles in null in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").nullified);
    }

    #[test]
    fn wormhole_size_parsed_from_words() {
        use crate::wormholes::ShipSize;
        let s = systems();
        let sz = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").wh_size;
        // "Extra large" (and xl / xlarge) is XL, and must beat the "large" substring.
        assert_eq!(sz("K162 nullsec extra large EOL"), Some(ShipSize::XLarge));
        assert_eq!(sz("wormhole xl to nullsec"), Some(ShipSize::XLarge));
        assert_eq!(sz("large wormhole in Rancer"), Some(ShipSize::Large));
        assert_eq!(sz("medium hole"), Some(ShipSize::Medium));
        assert_eq!(sz("frig hole in Rancer"), Some(ShipSize::Frigate));
        // Size is only read inside a wormhole message, so a normal gang report is unaffected.
        assert_eq!(sz("large gang in Rancer"), None);
    }

    #[test]
    fn wormhole_sig_not_duplicated_as_sig_badge() {
        let s = systems();
        // A wormhole sig shows on the wormhole badge, so it must not also raise a Sig badge.
        let r = analyze("sig ABC-123 wormhole to nullsec", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.wormhole, "should be a wormhole");
        assert_eq!(r.wh_sig.as_deref(), Some("ABC-123"), "wh_sig={:?}", r.wh_sig);
        assert!(
            !r.anom_sigs.iter().any(|(_, c)| c.eq_ignore_ascii_case("ABC-123")),
            "duplicate Sig badge: {:?}",
            r.anom_sigs
        );
        // A non-wormhole signature still raises its Sig badge.
        let r2 = analyze("sig XYZ-456 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            r2.anom_sigs.iter().any(|(_, c)| c.eq_ignore_ascii_case("XYZ-456")),
            "anom_sigs={:?}",
            r2.anom_sigs
        );
    }

    #[test]
    fn sisters_combat_scanner_is_probes_not_pilots() {
        let s = systems();
        let r = analyze("Sisters Combat Scanner in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.probes, Some(Probes::Combat), "probes={:?}", r.probes);
        assert!(esi_resolve(&r.pilots, &[]).is_empty(), "pilots={:?}", r.pilots);
    }

    #[test]
    fn drops_subphrase_pilots_works() {
        let mut p = vec!["Nine".to_string(), "Nine -3".to_string()];
        drop_subphrase_pilots(&mut p, &std::collections::HashSet::new(), "Nine -3");
        assert_eq!(p, vec!["Nine -3".to_string()]);
        let mut q = vec!["Callas Plaude".to_string(), "Callas Plaude Wolf".to_string()];
        let protect: std::collections::HashSet<String> = ["callas plaude".to_string()].into();
        drop_subphrase_pilots(&mut q, &protect, "Callas Plaude Wolf");
        assert!(q.contains(&"Callas Plaude".to_string()), "q={q:?}");
        let mut t = vec!["Tiffanbrill".to_string(), "Tiffanbrill Dragon".to_string()];
        drop_subphrase_pilots(
            &mut t,
            &std::collections::HashSet::new(),
            "Tiffanbrill Tiffanbrill Dragon",
        );
        assert_eq!(t, vec!["Tiffanbrill".to_string(), "Tiffanbrill Dragon".to_string()], "t={t:?}");
        let mut u = vec!["Ruston Shackleford".to_string(), "Ruston Shackleford B-3QPD".to_string()];
        drop_subphrase_pilots(
            &mut u,
            &std::collections::HashSet::new(),
            "Ruston Shackleford B-3QPD",
        );
        assert_eq!(u, vec!["Ruston Shackleford B-3QPD".to_string()], "u={u:?}");
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
        let (pilots, _, _) = resolve_report(&r, &["Psychopathic beemaster"], &s);
        assert_eq!(pilots, vec!["Psychopathic beemaster".to_string()], "pilots={pilots:?}");
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
        let known: std::collections::HashMap<String, i64> =
            [("navy".to_string(), 1i64), ("comet".to_string(), 2i64)].into_iter().collect();
        let r = analyze("Federation Navy Comet Docteur West in Rancer", &s, &ships, &known, 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Navy")), "pilots={:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Comet")), "pilots={:?}", r.pilots);
    }

    #[test]
    fn hedging_think_not_a_pilot_even_if_known() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("think".to_string(), 1i64)].into_iter().collect();
        let r = analyze("i think Sevra is in Rancer", &s, &noships(), &known, 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["Sevra"], &s);
        assert!(!pilots.iter().any(|p| p.eq_ignore_ascii_case("think")), "pilots={pilots:?}");
        assert!(pilots.iter().any(|p| p == "Sevra"), "pilots={pilots:?}");
    }

    #[test]
    fn content_keyword_kept_inside_name() {
        let s = systems();
        let r = analyze("High Plains Drifter in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["High Plains Drifter"], &s);
        assert_eq!(pilots, vec!["High Plains Drifter".to_string()], "pilots={pilots:?}");
    }

    #[test]
    fn other_side_and_theft_are_not_pilots() {
        let s = systems();
        for m in ["Other Side in Jita", "skyhook Theft in Jita", "Other Side gang in Jita"] {
            let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(
                !r.pilots.iter().any(|p| ["side", "other", "theft"].contains(&p.to_lowercase().as_str())),
                "{m} -> spurious pilot: {:?}",
                r.pilots
            );
        }
    }

    #[test]
    fn mid_name_connector_keeps_name_whole() {
        let s = systems();
        for (m, want) in [
            ("Cult is Dead in Rancer", "Cult is Dead"),
            ("Lord of War in Rancer", "Lord of War"),
        ] {
            let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(r.pilots.iter().any(|p| p == want), "{m} -> {:?}", r.pilots);
            assert!(!r.pilots.iter().any(|p| p == "Cult" || p == "Dead" || p == "War"), "{m} -> {:?}", r.pilots);
        }
        let r = analyze("Sevra is in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p == "Sevra is"), "pilots={:?}", r.pilots);
        let r2 = analyze("gate is camped in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.is_empty(), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn uppercase_x_multiplier_is_a_count() {
        let s = systems();
        for m in ["x5 in Rancer", "X5 in Rancer", "X12 hostiles Rancer"] {
            let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(r.count.is_some(), "{m} -> count {:?}", r.count);
        }
        assert_eq!(analyze("X5 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").count, Some(5));
        assert_eq!(analyze("x5 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").count, Some(5));
    }

    #[test]
    fn skyhook_typo_still_detected() {
        let s = systems();
        let r = analyze("skhook theft in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.skyhook, "skyhook flag not set: {:?}", r.text);
        assert!(r.structures.iter().any(|(n, _)| n.as_str() == "Skyhook"), "structs={:?}", r.structures);
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
        let r2 = analyze("Schook in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r2.skyhook, "schook wrongly flagged as skyhook");
    }

    #[test]
    fn descriptor_and_verb_words_are_not_pilots() {
        let s = systems();
        let r = analyze("Sevra jumped Navy Issue in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        let resolved = esi_resolve(&r.pilots, &["Sevra"]);
        assert_eq!(resolved, vec!["Sevra".to_string()], "resolved={resolved:?} from {:?}", r.pilots);
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
        let p1 = analyze("planet 1 Jita", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(p1.celestials, vec!["Planet 1".to_string()]);
        assert!(p1.count.is_none(), "count={:?}", p1.count);
        let m = analyze("moon IV in Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(m.celestials, vec!["Moon IV".to_string()]);
        let m53 = analyze("moon 5-3 Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(m53.celestials, vec!["Moon 5-3".to_string()]);
        assert!(m53.count.is_none(), "count={:?}", m53.count);
        let paste = analyze("Rancer VI - Moon 12", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(paste.celestials, vec!["Moon 6-12".to_string()], "cels={:?}", paste.celestials);
        let mi = analyze("moon I think it's clear Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert!(mi.celestials.is_empty(), "phantom celestial: {:?}", mi.celestials);
        let sun = analyze("camped at the sun Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(sun.celestials, vec!["Sun".to_string()]);
        assert_eq!(detect_structures("POS bash Rancer"), vec![("POS".to_string(), None)]);
        assert!(is_structure_word("pos"));
        assert!(detect_structures("hostiles in Rancer").is_empty());
        assert!(is_structure_word("fort") && is_structure_word("keep") && is_structure_word("astra"));
        let cb = analyze("Cyno Beacon online in Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert!(cb.structures.iter().any(|(n, _)| n == "Cyno Beacon"), "structures={:?}", cb.structures);
        assert!(
            !cb.pilots.iter().any(|p| p.eq_ignore_ascii_case("beacon") || p.eq_ignore_ascii_case("cyno")),
            "structure word leaked as a pilot: {:?}",
            cb.pilots
        );
    }

    #[test]
    fn scanner_probes_badge_not_ship_or_pilot() {
        assert_eq!(detect_probes("Sisters Core Scanner Probe on dscan"), Some(Probes::Core));
        assert_eq!(detect_probes("Combat Scanner Probe I"), Some(Probes::Combat));
        assert_eq!(detect_probes("Core Probes"), Some(Probes::Core));
        assert_eq!(detect_probes("combat probes out"), Some(Probes::Combat));
        assert_eq!(detect_probes("probes on dscan"), Some(Probes::Any));
        assert_eq!(detect_probes("Probe tackled"), None);
        assert_eq!(detect_probes("hostiles in Rancer"), None);

        let si =
            std::collections::HashMap::from([("probe".to_string(), (587i64, "Probe".to_string()))]);
        let s = systems();
        let r = analyze("Sisters Core Scanner Probe on dscan", &s, &si, &noknown(), 1, "ch", "x");
        assert_eq!(r.probes, Some(Probes::Core));
        assert!(r.ships.iter().all(|sh| !sh.name.eq_ignore_ascii_case("probe")), "{:?}", r.ships);
        assert!(
            !r.pilots.iter().any(|p| p.to_lowercase().contains("probe")),
            "{:?}",
            r.pilots
        );
        let r2 = analyze("Probe tackled", &s, &si, &noknown(), 1, "ch", "x");
        assert!(r2.ships.iter().any(|sh| sh.name.eq_ignore_ascii_case("probe")));
        assert!(analyze("prob cyno in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").probes.is_none());
        assert_eq!(analyze("combat probes on dscan", &s, &noships(), &noknown(), 1, "ch", "x").probes, Some(Probes::Combat));
        let rp = analyze("RSS Scanner Probe tackled in Rancer", &s, &si, &noknown(), 1, "ch", "x");
        assert_eq!(rp.probes, None, "pilot name triggered a probe badge: {:?}", rp.probes);
        assert!(
            rp.pilots.iter().any(|p| p == "RSS Scanner Probe"),
            "RSS Scanner Probe not a pilot: {:?}",
            rp.pilots
        );
        assert_eq!(
            analyze("RSS Scanner Probe and Sisters Combat Scanner Probe on dscan", &s, &si, &noknown(), 1, "ch", "x").probes,
            Some(Probes::Combat),
            "real probes after the pilot name should still fire"
        );
    }

    #[test]
    fn parses_isk_amounts() {
        assert_eq!(parse_isk("ess 300kk 5 min", true), Some(300_000_000));
        assert_eq!(parse_isk("ess worth 1.5b", true), Some(1_500_000_000));
        assert_eq!(parse_isk("ess 300 mil tag", true), Some(300_000_000));
        assert_eq!(parse_isk("worth 1.5b", false), None);
        assert_eq!(parse_isk("300 mil tag", false), None);
        assert_eq!(parse_isk("ess 750m", true), Some(750_000_000));
        assert_eq!(parse_isk("loot 750m", false), None);
        assert_eq!(parse_isk("ess hostiles in 4M-HGW", true), None);
        assert_eq!(parse_isk("5 min", false), None);
        assert_eq!(parse_isk("Rancer 3 Drake +2", false), None);
        assert_eq!(parse_isk("ess robbed 30m", true), None);
        assert_eq!(parse_isk("ess reserve 30m bank", true), None);
        assert_eq!(parse_isk("ess 50m", true), Some(50_000_000));
        assert_eq!(parse_isk("ess 77m bank", true), Some(77_000_000));
        assert_eq!(parse_isk("30m loot", false), None);
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
        assert!(extract_pilots("384-IN The Meek").iter().any(|r| r == "The Meek"));
    }

    #[test]
    fn intel_descriptor_breaks_a_name_run() {
        let out = extract_pilots("Cloaked Predator");
        assert!(!out.iter().any(|r| r.to_lowercase().contains("cloaked")), "out={:?}", out);
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
        let r = analyze("Sevra in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn keywords_no_substring_false_trigger() {
        let s = systems();
        let r = analyze("Bunk Boi Bunk Helper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r.help, "Helper must not trigger help");
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("cynabal".to_string(), (17720i64, "Cynabal".to_string()))].into_iter().collect();
        let r2 = analyze("Cynabal in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(!r2.cyno, "Cynabal must not trigger cyno");
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
        let r = analyze("bigfoott Kepplet in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("bigfoott Kepplet")),
            "pilots={:?}",
            r.pilots
        );
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
        let r = analyze("DZ Sharisa > 击杀：Wolf E Kristjansson", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.killmail, "should flag a kill from the Chinese keyword");
    }

    #[test]
    fn detects_single_ship_name() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
        let r = analyze("E-JCUS sabre", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(r.ships.iter().map(|sh| sh.name.clone()).collect::<Vec<_>>(), vec!["Sabre"]);
        let r2 = analyze("Sabre Smith in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r2.ships.is_empty());
        assert_eq!(r2.systems.iter().map(|d| d.name.clone()).collect::<Vec<_>>(), vec!["Rancer"]);
    }

    #[test]
    fn extracts_pilot_candidates() {
        let s = systems();
        let r = analyze("Some Pilot tackled in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(esi_resolve(&r.pilots, &["Some Pilot"]), vec!["Some Pilot".to_string()]);
        let r2 = analyze("Gate Camp in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(esi_resolve(&r2.pilots, &[]).is_empty(), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn amend_merges_ship_when_system_held_in_name_blob() {
        let s = systems();
        let sh: std::collections::HashMap<String, (i64, String)> =
            [("gila".to_string(), (17715i64, "Gila".to_string()))].into_iter().collect();
        let orig = analyze("C-J6MT Keeves nv", &s, &sh, &noknown(), 100, "ch", "Super Logico");
        let amend = analyze("Keeves C-J6MT Gila ?", &s, &sh, &noknown(), 140, "ch", "Yhana Malkav 2");
        let mut state = IntelState::default();
        state.push(orig);
        assert!(state.try_amend(&amend, 60, &s), "should amend on the shared pilot word");
        assert_eq!(state.reports.len(), 1);
        assert!(
            state.reports[0].ships.iter().any(|x| x.name == "Gila"),
            "Gila not merged: {:?}",
            state.reports[0].ships
        );
    }

    #[test]
    fn leading_digit_pilot_name_is_consistent_and_amends() {
        let s = systems_with(&[("kzfv-4", "KZFV-4", 30100, -0.5)]);
        let ships = ships_with(&[("Exequror Navy Issue", 29344)]);
        let known: std::collections::HashMap<String, i64> =
            [("1 tap machine".to_string(), 1i64)].into_iter().collect();
        let a = analyze("1 Tap Machine ENI", &s, &ships, &known, 100, "ch", "Corn SilkTea");
        let b = analyze("KZFV-4* 1 Tap Machine", &s, &ships, &known, 130, "ch", "jhouzy");
        assert!(proposed(&a.pilots, "1 Tap Machine"), "A pilots={:?}", a.pilots);
        assert!(proposed(&b.pilots, "1 Tap Machine"), "B pilots={:?}", b.pilots);
        assert_eq!(a.count, None, "leading digit counted in A: {:?}", a.pilots);
        assert_eq!(b.count, None, "leading digit counted in B: {:?}", b.pilots);
        assert!(b.systems.iter().any(|d| d.name == "KZFV-4"), "B system={:?}", b.systems);
        let mut state = IntelState::default();
        state.push(a);
        assert!(state.try_amend(&b, 60, &s), "second mention should amend the first");
        assert_eq!(state.reports.len(), 1, "split into separate cards: {:?}", state.reports);
        assert!(state.reports[0].systems.iter().any(|d| d.name == "KZFV-4"), "system not merged");

        let drake = ships_with(&[("Drake", 24698)]);
        let c = analyze("3 Drake", &s, &drake, &noknown(), 1, "ch", "x");
        assert_eq!(c.count, Some(3), "3 Drake should be a count: {:?}", c);
        assert!(!proposed(&c.pilots, "3 Drake"), "3 Drake leaked as a pilot: {:?}", c.pilots);
    }

    #[test]
    fn wilen_amend_keeps_full_three_word_name() {
        let s = systems();
        let ships = ships_with(&[("Stabber", 622)]);
        let known: std::collections::HashMap<String, i64> =
            [("elizabeth van wilen".to_string(), 1i64)].into_iter().collect();
        let a =
            analyze("Rancer Elizabeth van Wilen", &s, &noships(), &known, 100, "ch", "Savant Solette");
        let b =
            analyze("Elizabeth van Wilen Stabber", &s, &ships, &known, 130, "ch", "Jeff Kali");
        assert!(proposed(&a.pilots, "Elizabeth van Wilen"), "A pilots={:?}", a.pilots);
        assert!(proposed(&b.pilots, "Elizabeth van Wilen"), "B pilots={:?}", b.pilots);
        assert!(
            !b.pilots.iter().any(|p| p.eq_ignore_ascii_case("Wilen") || p.eq_ignore_ascii_case("Wilen Stabber")),
            "B leaked bare 'Wilen': {:?}",
            b.pilots
        );
        let mut state = IntelState::default();
        state.push(a);
        assert!(state.try_amend(&b, 60, &s), "second mention should amend the first");
        assert_eq!(state.reports.len(), 1, "split into separate cards: {:?}", state.reports);
        assert!(
            proposed(&state.reports[0].pilots, "Elizabeth van Wilen"),
            "merged pilots={:?}",
            state.reports[0].pilots
        );
        assert!(
            !state.reports[0].pilots.iter().any(|p| p.eq_ignore_ascii_case("Wilen") || p.eq_ignore_ascii_case("Wilen Stabber")),
            "merged leaked bare 'Wilen': {:?}",
            state.reports[0].pilots
        );
    }

    #[test]
    fn kill_paste_extracts_victim_and_ship() {
        let s = systems();
        let ships = ships_with(&[("Loki", 29990)]);
        let known: std::collections::HashMap<String, i64> =
            [("lord road".to_string(), 1i64), ("road".to_string(), 2i64)].into_iter().collect();
        let r = analyze("Kill: Lord Road (Loki)", &s, &ships, &known, 1, "ch", "x");
        assert!(r.killmail, "killmail flag");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "victim: {:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")), "bare Road: {:?}", r.pilots);
        assert!(r.ships.iter().any(|sh| sh.name == "Loki"), "ship: {:?}", r.ships);
        let r2 = analyze("击杀：Lord Road (洛基级)", &s, &noships(), &known, 1, "ch", "x");
        assert!(r2.killmail);
        assert!(r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "victim: {:?}", r2.pilots);
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")), "bare Road: {:?}", r2.pilots);
    }

    #[test]
    fn kill_paste_amend_no_double_consume() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("lord road".to_string(), 1i64), ("road".to_string(), 2i64)].into_iter().collect();
        let m1 = analyze("击杀：Lord Road (洛基级)  Rancer", &s, &noships(), &known, 100, "ch", "yuyexf");
        let m2 = analyze("击杀：Lord Road (洛基级)", &s, &noships(), &known, 130, "ch", "Aurelius Caracalla");
        let mut st = IntelState::default();
        st.push(m1);
        assert!(st.try_amend(&m2, 60, &s), "should amend on the shared victim");
        let merged = &st.reports[0].pilots;
        assert!(merged.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "victim: {:?}", merged);
        assert!(!merged.iter().any(|p| p.eq_ignore_ascii_case("Road")), "double-consumed Road: {:?}", merged);
    }

    #[test]
    fn paste_linked_dictionary_name_survives() {
        let s = systems();
        let r = analyze("fibular  detective spider  Q-K2T7", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("fibular")), "dropped: {:?}", r.pilots);
        let known: std::collections::HashMap<String, i64> =
            [("fibular".to_string(), 5i64)].into_iter().collect();
        let r2 = analyze("fibular in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("fibular")), "prose kept: {:?}", r2.pilots);
    }

    #[test]
    fn keyword_in_pasted_name_does_not_bail_paste() {
        let s = systems();
        let r = analyze(
            "Bsjsisnjs  buzuo333  detective spider  feng fenghua  fliet98 cyno  Q-K2T7",
            &s, &noships(), &noknown(), 1, "ch", "Muchchi",
        );
        for name in ["Bsjsisnjs", "buzuo333", "detective spider", "feng fenghua"] {
            assert!(
                r.pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
                "missing {name}: {:?}",
                r.pilots
            );
        }
        assert!(
            !r.pilots.iter().any(|p| p.split_whitespace().count() > 3),
            "glued blob leaked: {:?}",
            r.pilots
        );
    }

    #[test]
    fn subname_not_reused_when_full_name_resolved() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("lord road".to_string(), 1i64), ("road".to_string(), 2i64), ("capitaine onaga".to_string(), 3i64)]
                .into_iter()
                .collect();
        let r = analyze("Lord Road in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "pilots={:?}", r.pilots);
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")),
            "bare 'Road' leaked: {:?}",
            r.pilots
        );
        let ships = ships_with(&[("Nereus", 650)]);
        let r2 = analyze(
            "JV1V-O  Kill: Capitaine Onaga (Nereus)  Lord Road he's happy now",
            &s, &ships, &known, 1, "ch", "Capitaine Onaga",
        );
        assert!(
            !r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")),
            "bare 'Road' leaked from kill paste: {:?}",
            r2.pilots
        );
    }

    #[test]
    fn amends_successive_reporter_messages() {
        let s = systems();
        let mut state = IntelState::default();
        state.push(analyze("hostile in Rancer", &s, &noships(), &noknown(), 100, "ch", "Scout"));
        let follow = analyze("on 78- gate", &s, &noships(), &noknown(), 130, "ch", "Scout");
        assert!(state.try_amend(&follow, 60, &s));
        assert_eq!(state.reports.len(), 1);
        assert!(!state.reports[0].gates.is_empty());
        let other = analyze("hostile in Jita", &s, &noships(), &noknown(), 140, "ch", "Scout");
        assert!(!state.try_amend(&other, 60, &s));
        let clear = analyze("Rancer clear", &s, &noships(), &noknown(), 150, "ch", "Scout");
        assert!(!state.try_amend(&clear, 60, &s));
    }

    #[test]
    fn clear_card_is_not_amended_by_later_sighting() {
        let s = systems();
        let mut state = IntelState::default();
        state.push(analyze("Rancer clear", &s, &noships(), &noknown(), 100, "ch", "Scout"));
        let follow = analyze("3 reds in Rancer", &s, &noships(), &noknown(), 120, "ch", "Scout");
        assert!(!state.try_amend(&follow, 60, &s));
        assert_eq!(state.reports.len(), 1);
        assert!(state.reports[0].clear);
    }

    #[test]
    fn known_pilots_match_with_subset_protection() {
        let s = systems();
        let k1: std::collections::HashMap<String, i64> =
            [("bigfoott".to_string(), 2i64)].into_iter().collect();
        let r = analyze("Rancer bigfoott", &s, &noships(), &k1, 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["bigfoott"], &s);
        assert!(pilots.iter().any(|p| p.eq_ignore_ascii_case("bigfoott")), "{pilots:?}");
        let k2: std::collections::HashMap<String, i64> =
            [("hold me balls".to_string(), 1i64), ("hold".to_string(), 3i64)].into_iter().collect();
        let r2 = analyze("E-JCUS HOLD ME BALLS", &s, &noships(), &k2, 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r2, &["hold me balls"], &s);
        assert!(pilots.iter().any(|p| p.eq_ignore_ascii_case("hold me balls")), "{pilots:?}");
        assert!(!pilots.iter().any(|p| p.eq_ignore_ascii_case("hold")), "{pilots:?}");
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
        let mut a = analyze("C-J6MT Pericle No1", &s, &noships(), &noknown(), 100, "ch", "Kobayashi Mika");
        apply_resolution(&mut a, &["Pericle No1"], &s);
        assert_eq!(a.pilots, vec!["Pericle No1".to_string()]);
        state.push(a);
        let mut follow = analyze("Pericle No1 loki", &s, &loki, &noknown(), 130, "ch", "Wallie Warptunnel");
        apply_resolution(&mut follow, &["Pericle No1"], &s);
        assert!(state.try_amend(&follow, 60, &s));
        assert_eq!(state.reports.len(), 1);
        assert!(state.reports[0].ships.iter().any(|sh| sh.name == "Loki"));
    }

    #[test]
    fn quoting_forces_pilot_not_keyword() {
        let s = systems();
        let r = analyze("'clear' in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "clear"));
        assert!(!r.clear);
        assert_eq!(r.systems.len(), 1);
        let r2 = analyze("`Some Guy\" tackled", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.iter().any(|p| p == "Some Guy"));
    }

    #[test]
    fn name_with_trailing_number_isnt_a_count() {
        let s = systems();
        assert_eq!(analyze("8X6T-8 Malcolm 41", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
        assert_eq!(analyze("Adama 80 pls help", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
        // A lone trailing number after a system is too error-prone to be a count.
        assert_eq!(analyze("Rancer 5", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
    }

    #[test]
    fn bare_numbers_need_a_qualifier() {
        let s = systems();
        let ships = ships_with(&[("Drake", 24698)]);
        let pos = |t: &str, ships: &std::collections::HashMap<String, (i64, String)>| {
            analyze(t, &s, ships, &noknown(), 1, "ch", "x").count
        };
        // Positive: a number qualified by +, x/X, a hostile keyword, or a ship counts.
        assert_eq!(pos("+3 in Rancer", &noships()), Some(3), "attached +N");
        assert_eq!(pos("3+ in Rancer", &noships()), Some(3), "attached N+");
        assert_eq!(pos("Rancer + 5", &noships()), Some(5), "+ before number, spaced");
        assert_eq!(pos("Rancer 5 +", &noships()), Some(5), "+ after number, spaced");
        assert_eq!(pos("x5 in Rancer", &noships()), Some(5), "x multiplier");
        assert_eq!(pos("X5 in Rancer", &noships()), Some(5), "X multiplier");
        assert_eq!(pos("5 reds in Rancer", &noships()), Some(5), "keyword after number");
        assert_eq!(pos("Rancer neuts 3", &noships()), Some(3), "keyword before number");
        assert_eq!(pos("hostiles 10 in Rancer", &noships()), Some(10), "hostile keyword");
        assert_eq!(pos("2 marauders in Rancer", &noships()), Some(2), "ship class");
        assert_eq!(pos("3 Drake in Rancer", &ships), Some(3), "known hull");
        assert_eq!(pos("2 Drakes in Rancer", &ships), Some(2), "plural hull");
        assert_eq!(pos("5 in system", &noships()), Some(5), "N in system");
        assert_eq!(pos("10 in local", &noships()), Some(10), "N in local");
        assert_eq!(pos("6 in sys", &noships()), Some(6), "N in sys");
        // Negative: a lone number, with no +/x/keyword/ship beside it, does not count.
        assert_eq!(pos("Rancer 5", &noships()), None, "lone trailing number");
        assert_eq!(pos("5 in Rancer", &noships()), None, "lone leading number");
        assert_eq!(pos("Rancer 5 gate", &noships()), None, "number between system and gate");
        assert_eq!(pos("camp in Rancer 8", &noships()), None, "stray number");
        assert_eq!(pos("3 Drake in Rancer", &noships()), None, "unknown word is not a ship");
    }

    #[test]
    fn loose_runs_keep_short_name_parts() {
        let s = systems();
        let runs = loose_pilot_runs("Adama 80 Lopatich R", &noships(), &s);
        assert!(runs.iter().any(|r| r.contains("80")), "runs={:?}", runs);
        assert!(runs.iter().any(|r| r.split_whitespace().last() == Some("R")), "runs={:?}", runs);
        assert!(loose_pilot_runs("80 90", &noships(), &s).is_empty());
    }

    #[test]
    fn system_detection_coverage() {
        let s = systems();
        let det = |m: &str| {
            let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
            (r.systems.iter().map(|x| x.name.clone()).collect::<Vec<String>>(), r.gates.clone())
        };
        assert_eq!(det("hostiles in Jita").0, vec!["Jita"]);
        assert_eq!(det("Jita").0, vec!["Jita"]);
        assert_eq!(det("5 reds Jita").0, vec!["Jita"]);
        assert_eq!(det("C-J6MT clear").0, vec!["C-J6MT"]);
        assert_eq!(det("Jita* hostiles").0, vec!["Jita"]);
        let (sysd, gates) = det("N3-JBX Uitra");
        assert_eq!(sysd, vec!["N3-JBX"]);
        assert!(gates.iter().any(|g| g == "Uitra"), "gates={gates:?}");
        assert!(det("on C-J gate").1.iter().any(|g| g == "C-J6MT"), "{:?}", det("on C-J gate"));
        assert_eq!(det("Sevra in Jita").0, vec!["Jita"]);
        assert!(det("hostiles incoming").0.is_empty());
        assert_eq!(det("c-j6mt clear").0, vec!["C-J6MT"]);
    }

    #[test]
    fn detects_systems_count_and_flags() {
        let s = systems();

        let drake = ships_with(&[("Drake", 24698)]);
        let r = analyze("hostile in Rancer, 3 Drake +2", &s, &drake, &noknown(), 100, "ch", "Scout");
        assert_eq!(r.systems.len(), 1);
        assert_eq!(r.systems[0].name, "Rancer");
        assert_eq!(r.count, Some(5));
        assert!(!r.clear);

        assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(analyze("nv in Jita", &s, &noships(), &noknown(), 1, "ch", "x").no_visual);
        assert!(analyze("gate camp 1DQ1-A bubble up", &s, &noships(), &noknown(), 1, "ch", "x").camp);
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
        for w in ["filament", "needlejack", "trace", "filaments", "needlejacks"] {
            let r = analyze(&format!("{w} in Rancer"), &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(r.filament, "{w} should set filament");
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w} is a keyword, not a pilot");
        }
        assert!(analyze("clear in here", &s, &noships(), &noknown(), 1, "ch", "x").systems.is_empty());
    }

    #[test]
    fn recognizes_battle_report_links_including_our_site() {
        let ours = extract_links("gf all https://eve-spai.com/br/abc123def nice fight");
        assert!(
            ours.iter().any(|l| l.kind == LinkKind::BattleReport && l.url.contains("eve-spai.com/br/")),
            "{ours:?}"
        );
        assert!(extract_links("https://br.evetools.org/br/xyz")
            .iter()
            .any(|l| l.kind == LinkKind::BattleReport));
        assert!(!extract_links("https://eve-spai.com/about").iter().any(|l| l.kind == LinkKind::BattleReport));
    }

    #[test]
    fn pronoun_i_never_a_pilot() {
        let s = systems();
        let has_i = |names: &[String]| names.iter().any(|p| p.split_whitespace().any(|w| w == "I"));
        for txt in [
            "I think 5 reds in Jita",
            "I guess they left",
            "tackled one, I saw him warp Jita",
            "Rancer clear, I am going afk",
            "dunno where they went, I missed it",
            "I see a Sabre and I think a Loki",
            "i think reds incoming",
            "Bishopi I think he docked",
            "warp to I and hold",
        ] {
            let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "Spai");
            let resolved = esi_resolve(&r.pilots, &["Bishopi", "Sabre"]);
            assert!(!has_i(&resolved), "pronoun 'I' leaked as a pilot in {txt:?}: {resolved:?}");
        }
        let r = analyze("Bishopi I think he docked", &s, &noships(), &noknown(), 1, "ch", "Spai");
        let resolved = esi_resolve(&r.pilots, &["Bishopi"]);
        assert_eq!(resolved, vec!["Bishopi".to_string()], "glued pronoun: {resolved:?}");
    }

    fn sys_map(rows: &[(&str, &str, i64, f64)]) -> std::collections::HashMap<String, SystemInfo> {
        rows.iter()
            .map(|(k, n, id, sec)| {
                (k.to_string(), SystemInfo { id: *id, name: n.to_string(), security: *sec, constellation: String::new(), region: String::new(), faction: String::new() })
            })
            .collect()
    }

    #[test]
    fn glued_oneword_handles_split_via_cache() {
        let s = Systems::new(sys_map(&[("9-ougj", "9-OUGJ", 30000454, -0.5)]), std::collections::HashMap::new());
        let known: std::collections::HashMap<String, i64> = [
            ("clol23".to_string(), 2124249172i64),
            ("rm712".to_string(), 2117556515),
            ("wenmg".to_string(), 2121075688),
        ]
        .into_iter()
        .collect();
        let plain = "clol23 MuskQAQ rm712 wenmg 9-OUGJ";
        let r = analyze(plain, &s, &noships(), &known, 1, "ch", "TreeBeard Elderling");
        let split = esi_resolve(&r.pilots, &["clol23", "MuskQAQ", "rm712", "wenmg"]);
        let lc: Vec<String> = split.iter().map(|p| p.to_lowercase()).collect();
        for want in ["clol23", "rm712", "wenmg", "muskqaq"] {
            assert!(lc.contains(&want.to_string()), "missing {want}: {:?}", split);
        }

        let known2: std::collections::HashMap<String, i64> =
            [("comet".to_string(), 90i64)].into_iter().collect();
        let r2 = analyze("Comet Rider in 9-OUGJ", &s, &noships(), &known2, 1, "ch", "x");
        let resolved = esi_resolve(&r2.pilots, &["Comet Rider"]);
        assert!(resolved.iter().any(|p| p.eq_ignore_ascii_case("Comet Rider")), "pilots: {resolved:?}");
        assert!(!resolved.iter().any(|p| p.eq_ignore_ascii_case("Rider")), "wrongly split: {resolved:?}");
    }

    #[test]
    fn double_space_paste_recognises_pilots() {
        let s = Systems::new(
            sys_map(&[
                ("l-fm3p", "L-FM3P", 30000540, -0.5),
                ("9-ougj", "9-OUGJ", 30000454, -0.5),
                ("ypw-m4", "YPW-M4", 30000785, -0.5),
            ]),
            std::collections::HashMap::new(),
        );
        let lc = |r: &IntelReport| {
            let mut v: Vec<String> = r.pilots.iter().map(|p| p.to_lowercase()).collect();
            v.sort();
            v
        };

        let r = analyze("L-FM3P  Gliar  Mliarvis  Sliarhia", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar", "mliarvis", "sliarhia"]);
        assert!(r.systems.iter().any(|d| d.name == "L-FM3P"), "system kept: {:?}", r.systems);

        let r = analyze("clol23  MuskQAQ  rm712  wenmg  9-OUGJ", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["clol23", "muskqaq", "rm712", "wenmg"]);

        let r = analyze("YPW-M4*  Boris95  BorisDread95  Destroyer95", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["boris95", "borisdread95", "destroyer95"]);
        assert!(r.systems.iter().any(|d| d.name == "YPW-M4"), "system: {:?}", r.systems);

        let r = analyze("L-FM3P  First Last  Second Guy", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["first last", "second guy"]);

        let r = analyze("L-FM3P    Gliar    Mliarvis", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar", "mliarvis"]);

        let known: std::collections::HashMap<String, i64> =
            [("gliar".to_string(), 1i64), ("mliarvis".to_string(), 2), ("sliarhia".to_string(), 3)]
                .into_iter()
                .collect();
        let r = analyze("L-FM3P  Gliar  Mliarvis  Sliarhia", &s, &noships(), &known, 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar", "mliarvis", "sliarhia"]);

        let ships: std::collections::HashMap<String, (i64, String)> =
            [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
        let r = analyze("L-FM3P  Gliar  Sabre", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar"], "sabre must not be a pilot");
        assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "sabre is a ship: {:?}", r.ships);
    }

    #[test]
    fn double_space_falls_back_on_prose_and_bad_grammar() {
        let s = systems();
        let pilots = |t: &str| {
            let mut v = analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").pilots;
            v.sort();
            v
        };
        assert!(
            analyze("rorqual  pointed in Jita", &s, &noships(), &noknown(), 1, "ch", "x").cap_tackled,
            "cap detection must survive a stray double space"
        );
        for t in [
            "reds  pointed in Jita",
            "they  warped off to Jita",
            "got him  tackled in Jita now",
            "Rancer  is clear now lads",
        ] {
            let resolved = esi_resolve(&pilots(t), &[]);
            assert!(resolved.is_empty(), "prose treated as paste for {t:?}: {resolved:?}");
        }
        for (dbl, sgl) in [
            ("reds  pointed in Jita", "reds pointed in Jita"),
            ("they  warped off to Jita", "they warped off to Jita"),
            ("lol  gg  wp", "lol gg wp"),
            ("he  said  hi", "he said hi"),
            ("idk  man  lol", "idk man lol"),
            ("u  see  them", "u see them"),
            ("ok  ok  sure", "ok ok sure"),
            ("cats  love  fish", "cats love fish"),
            ("nice  one  mate", "nice one mate"),
            ("Rancer  is clear now lads", "Rancer is clear now lads"),
        ] {
            assert_eq!(pilots(dbl), pilots(sgl), "double-space hint changed a non-paste parse: {dbl:?}");
        }
        let r = analyze("Rancer  Gliar  they all warped off already", &s, &noships(), &noknown(), 1, "ch", "x");
        let resolved = esi_resolve(&r.pilots, &["Gliar"]);
        assert_eq!(resolved, vec!["Gliar".to_string()], "prose tail leaked: {resolved:?}");
    }

    #[test]
    fn lowercase_clear_rain_pilot_detected() {
        let s = systems();
        let r = analyze("Rancer clear rain nemesis on gate", &s, &noships(), &noknown(), 1, "ch", "Spai");
        assert!(r.pilots.iter().any(|p| p == "clear rain"), "clear rain not a pilot: {:?}", r.pilots);
        assert!(!r.clear, "pilot name 'clear rain' spoofed a clear status");
        assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "Spai").clear);
    }

    #[test]
    fn clear_loses_to_threats() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("nemesis".to_string(), (11377i64, "Nemesis".to_string()))].into_iter().collect();
        let r = analyze("Rancer hot dropper bubble clear rain nemesis", &s, &ships, &noknown(), 1, "ch", "Spai");
        assert!(!r.clear, "clear should lose to threats");
        let r2 = analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "Spai");
        assert!(r2.clear, "pure clear lost");
        let r3 = analyze("got Clear Rain on gate", &s, &noships(), &noknown(), 1, "ch", "Spai");
        let (pilots, _, _) = resolve_report(&r3, &["Clear Rain"], &s);
        assert!(pilots.iter().any(|p| p == "Clear Rain"), "name split: {pilots:?}");
        assert!(!r3.clear, "name 'Clear Rain' spoofed clear");
    }

    #[test]
    fn different_main_systems_do_not_amend_on_shared_pilot() {
        let by_name = [("rz-ti6", "RZ-TI6", 30000834i64, -0.4), ("fx4l-2", "FX4L-2", 30000835, -0.4)]
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
        let s = Systems::new(by_name, HashMap::new());
        let mut state = IntelState::default();
        let mut a =
            analyze("RZ-TI6  Cloister Cobon-Han", &s, &noships(), &noknown(), 100, "ch", "BiGsnorlax");
        apply_resolution(&mut a, &["Cloister Cobon-Han"], &s);
        assert_eq!(a.primary_system().map(|d| d.id), Some(30000834), "msg1 system");
        state.push(a);
        let mut b = analyze(
            "Cloister Cobon-Han  FX4L-2 imucs",
            &s,
            &noships(),
            &noknown(),
            130,
            "ch",
            "utsumi ota",
        );
        apply_resolution(&mut b, &["Cloister Cobon-Han"], &s);
        assert_eq!(b.primary_system().map(|d| d.id), Some(30000835), "msg2 system");
        assert!(!state.try_amend(&b, 60, &s), "different main systems must not amend");
        assert_eq!(state.reports.len(), 1, "the two sightings stay separate");
    }

    #[test]
    fn pasted_name_sharing_a_word_with_a_system_still_resolves() {
        let by_name = [("moh", "Moh", 30000750i64, 0.3), ("4ds-oi", "4DS-OI", 30000749, -0.5)]
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
        let s = Systems::new(by_name, HashMap::new());
        for t in ["4DS-OI  Moh Lut nv", "Moh Lut  4DS-OI nv core probes out"] {
            let r = analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(r.pilots.iter().any(|p| p == "Moh Lut"), "{t}: pilots={:?}", r.pilots);
            assert!(r.systems.iter().any(|d| d.name == "4DS-OI"), "{t}: systems={:?}", r.systems);
            assert!(
                !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Moh")),
                "{t}: 'Moh' leaked as a pilot: {:?}",
                r.pilots
            );
        }
    }

    #[test]
    fn pilot_name_with_keyword_words_and_trailing_ship_note() {
        let s = systems();
        let ships = ships_with(&[("Prospect", 33468)]);
        let known: std::collections::HashMap<String, i64> =
            [("roadman highsec cynolighter".to_string(), 1i64)].into_iter().collect();
        for t in ["DUO-51  Roadman HighSec CynoLighter likely prospect"] {
            let r = analyze(t, &s, &ships, &known, 1, "ch", "Rage Starscythe");
            assert!(
                r.pilots.iter().any(|p| p == "Roadman HighSec CynoLighter"),
                "{t}: pilots={:?}",
                r.pilots
            );
            assert!(
                !r.pilots.iter().any(|p| {
                    p.to_lowercase().contains("likely") || p.to_lowercase().contains("prospect")
                }),
                "{t}: leaked prose/ship into a pilot: {:?}",
                r.pilots
            );
            assert!(r.ships.iter().any(|sh| sh.name == "Prospect"), "{t}: ships={:?}", r.ships);
        }
    }

    #[test]
    fn gate_variants_nameless_gate_and_solo() {
        let s = systems();
        let r = analyze("1DQ1-A camp gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.camp, "camp keyword");
        assert!(r.gates.iter().any(|g| g.is_empty()), "nameless gate expected: {:?}", r.gates);
        assert!(
            !r.gates.iter().any(|g| g.eq_ignore_ascii_case("camp")),
            "camp captured as gate: {:?}",
            r.gates
        );
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("camp")), "camp pilot: {:?}", r.pilots);
        for kw in ["gates", "stargate", "stargates"] {
            let r = analyze(&format!("Rancer {kw} clear"), &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(kw)), "{kw} pilot: {:?}", r.pilots);
        }
        let r = analyze("Rancer solo Sabre", &s, &ships_with(&[("Sabre", 22456)]), &noknown(), 1, "ch", "x");
        assert_eq!(r.count, Some(1), "solo count: {:?}", r.count);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("solo")), "solo pilot: {:?}", r.pilots);
    }

    #[test]
    fn nvm_camper_whoever_are_not_pilots() {
        let s = systems();
        let r = analyze("Rancer nvm", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("nvm")), "nvm: {:?}", r.pilots);
        let r = analyze("Rancer campers on gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.camp, "campers should fire camp");
        assert!(
            !r.pilots.iter().any(|p| p.to_lowercase().contains("camper")),
            "camper as pilot: {:?}",
            r.pilots
        );
        let r = analyze("Whoever is tackling in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(
            !r.pilots.iter().any(|p| p.eq_ignore_ascii_case("whoever")),
            "whoever as pilot: {:?}",
            r.pilots
        );
    }

    #[test]
    fn pluralised_multiword_and_ies_hulls() {
        let s = systems();
        let ships = ships_with(&[("Osprey Navy Issue", 29990), ("Osprey", 620), ("Harpy", 11381)]);
        for t in ["osprey navys in Rancer", "osprey navies in Rancer"] {
            let r = analyze(t, &s, &ships, &noknown(), 1, "ch", "x");
            assert!(r.ships.iter().any(|sh| sh.name == "Osprey Navy Issue"), "{t}: {:?}", r.ships);
            assert!(
                !r.pilots.iter().any(|p| {
                    p.eq_ignore_ascii_case("navys") || p.eq_ignore_ascii_case("navies")
                }),
                "{t} pilot: {:?}",
                r.pilots
            );
        }
        let r = analyze("harpies on grid", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Harpy"), "{:?}", r.ships);
        let ships2 = ships_with(&[("Ares", 11196), ("Bellicose", 29344)]);
        let r = analyze("areses on grid", &s, &ships2, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Ares"), "areses: {:?}", r.ships);
        let r = analyze("bellicoses on grid", &s, &ships2, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Bellicose"), "bellicoses: {:?}", r.ships);
    }

    #[test]
    fn plural_sabres_and_on_grid_not_pilots() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
        let r = analyze("Rancer 5 Sabres on grid", &s, &ships, &noknown(), 1, "ch", "Spai");
        assert!(!r.pilots.iter().any(|p| p.to_lowercase().contains("sabres")), "sabres as pilot: {:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.to_lowercase().contains("grid")), "grid as pilot: {:?}", r.pilots);
        assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "Sabre ship missing: {:?}", r.ships);
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
    fn pilot_name_keeps_alt_suffix() {
        let s = systems();
        let r = analyze("hostiles Nine -L in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["Nine -L"], &s);
        assert!(pilots.iter().any(|p| p == "Nine -L"), "pilots: {pilots:?}");
        let r2 = analyze("Nine -3", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r2, &["Nine -3"], &s);
        assert!(pilots.iter().any(|p| p == "Nine -3"), "pilots: {pilots:?}");
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
        let (pilots, _, _) = resolve_report(&r, &["Psychopathic beemaster"], &s);
        assert!(pilots.iter().any(|p| p == "Psychopathic beemaster"), "{pilots:?}");
    }

    #[test]
    fn numbered_name_with_system_prefix_plain_text() {
        let s = systems();
        let r = analyze("SV5-8N Amarr slave 3424", &s, &noships(), &noknown(), 1, "ch", "x");
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
        let r2 = analyze("Jita N968 sig", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.wh_type.as_deref(), Some("N968"));
        let r3 = analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r3.wormhole);
        assert!(r3.wh_type.is_none());
    }

    #[test]
    fn neighbour_second_system_becomes_gate() {
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
    fn non_adjacent_second_system_is_not_a_gate() {
        let by_name: std::collections::HashMap<String, SystemInfo> = [
            ("r959-u", "R959-U", 1, -0.2),
            ("agaullores", "Agaullores", 2, 0.3),
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
        let adj = std::collections::HashMap::from([(1i64, vec![99]), (2, vec![99])]);
        let s = Systems::new(by_name, adj);
        let r = analyze("R959-U WH to Agaullores", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["R959-U"]);
        assert!(r.gates.is_empty(), "non-adjacent WH destination wrongly demoted to a gate: {:?}", r.gates);
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
        let q = analyze("status in Rancer?", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(q.status);
        assert!(!q.pilots.iter().any(|p| p.eq_ignore_ascii_case("status")));
    }

    #[test]
    fn lowercase_english_word_dropped_but_names_and_multiword_kept() {
        let s = systems();
        let known: std::collections::HashMap<String, i64> =
            [("carpet".to_string(), 100i64), ("silent hunter".to_string(), 200i64)]
                .into_iter()
                .collect();
        let low = analyze("carpet in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert!(
            !low.pilots.iter().any(|p| p.eq_ignore_ascii_case("carpet")),
            "lowercase word should be dropped, pilots={:?}",
            low.pilots
        );
        let cap = analyze("Carpet in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert!(
            cap.pilots.iter().any(|p| p.eq_ignore_ascii_case("carpet")),
            "Capitalised name should be kept, pilots={:?}",
            cap.pilots
        );
        let multi = analyze("silent hunter in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert!(
            multi.pilots.iter().any(|p| p.eq_ignore_ascii_case("silent hunter")),
            "multi-word lowercase run should still be tested, pilots={:?}",
            multi.pilots
        );
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
        let adj = HashMap::from([(5i64, vec![10i64, 9]), (10, vec![5]), (9, vec![5])]);
        let s = Systems::new(by_name, adj);
        let r = analyze("C-J6MT 5e gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("5E-CFL"));
        let r2 = analyze("C-J6MT sv gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("SV5-8N"));
    }

    #[test]
    fn gate_disambiguates_abbrev_via_context() {
        use std::collections::HashMap;
        let by_name = [
            ("d-pnsn", "D-PNSN", 1i64, -0.4),
            ("c-j6mt", "C-J6MT", 2, -0.6),
            ("c-jeez", "C-JEEZ", 3, -0.5),
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
        let adj = HashMap::from([(1i64, vec![2i64]), (2, vec![1])]);
        let s = Systems::new(by_name, adj);
        let r = analyze("D-PNSN C-J gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
        let r2 = analyze_ctx("C-J gate", &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
        assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
    }

    #[test]
    fn detects_cap_tackled_variations() {
        let s = systems();
        let cap = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").cap_tackled;
        assert!(cap("Rancer cap tackled"));
        assert!(cap("rorqual  pointed in Jita"));
        assert!(cap("dread scrammed on gate"));
        assert!(cap("carrier takled"));
        assert!(cap("super got scram"));
        assert!(!cap("cap stable"));
        assert!(!cap("tackled a frigate"));
    }

    #[test]
    fn ess_time_ignores_isk_amount() {
        let s = systems();
        let r = analyze("TPG-DD ESS 5 min 77m bank", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.ess_time.as_deref(), Some("5m"));
        let r2 = analyze("ESS reserve 30 min", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.ess_time.as_deref(), Some("30m"));
        let r3 = analyze("ESS robbed 30m", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r3.ess_time, None);
        let r4 = analyze("ESS robbed 30seconds left", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r4.ess_time.as_deref(), Some("30s"));
        assert!(
            !r4.pilots.iter().any(|p| p.to_lowercase().contains("30seconds")),
            "time token leaked as a pilot: {:?}",
            r4.pilots
        );
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
        let r = analyze("C-J +20 on 78- gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
        assert_eq!(r.count, Some(20));
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("78-AAA"));
        assert_eq!(
            r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["C-J6MT"],
        );

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

    fn systems_with(extra: &[(&str, &str, i64, f64)]) -> Systems {
        let mut by_name: std::collections::HashMap<String, SystemInfo> = std::collections::HashMap::new();
        for (key, name, id, sec) in extra {
            by_name.insert(
                key.to_string(),
                SystemInfo {
                    id: *id,
                    name: name.to_string(),
                    security: *sec,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            );
        }
        Systems::new(by_name, HashMap::new())
    }

    #[test]
    fn parse_isk_handles_mio_million_abbreviation() {
        assert_eq!(parse_isk("ess 346mio", true), Some(346_000_000));
        assert_eq!(parse_isk("346mio", false), None);
        assert_eq!(parse_isk("ess worth 120 mio", true), Some(120_000_000));
        assert_eq!(parse_isk("ess 50mio.", true), Some(50_000_000));
        assert_eq!(parse_isk("loot 750m", false), None);
        assert_eq!(parse_isk("ess hostiles in 4M-HGW", true), None);
    }

    #[test]
    fn htg0_is_a_pilot_mskr1_stays_a_system() {
        let s = systems_with(&[("mskr-1", "MSKR-1", 99, -0.5)]);
        let ships = ships_with(&[("Gnosis", 3756), ("Slasher", 585)]);
        let r = analyze(
            "MSKR-1 Htg-0 +5 gnosis 3x, Slasher, ESS 346mio",
            &s, &ships, &noknown(), 1, "ch", "Duke Dekker",
        );
        assert!(r.ships.iter().any(|sh| sh.name == "Gnosis"), "ships={:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Slasher"), "ships={:?}", r.ships);
        assert!(r.ess, "ESS flag should fire: {:?}", r.text);
        assert_eq!(r.isk, Some(346_000_000), "isk={:?}", r.isk);
        assert!(!has_pilot_token(&r.pilots, "346mio"), "amount leaked as pilot: {:?}", r.pilots);
        assert!(proposed(&r.pilots, "Htg-0"), "Htg-0 not proposed: {:?}", r.pilots);
        let (pilots, sysd, gates) = resolve_report(&r, &["Htg-0"], &s);
        assert_eq!(pilots, vec!["Htg-0".to_string()], "resolved pilots={pilots:?}");
        assert_eq!(sysd, vec!["MSKR-1".to_string()], "resolved systems={sysd:?}");
        assert!(gates.is_empty(), "gates={gates:?}");
    }

    #[test]
    fn code_pattern_name_without_real_system_is_a_pilot() {
        for t in ["Htg-0", "htg-0", "HTG-0", "MSKR-1", "Zzz-9", "zzz-9"] {
            assert!(looks_like_system_code(t), "{t} should match the code pattern");
        }
        assert!(!looks_like_system_code("Jean-Luc"), "a long-segment name is not a code");
        let s = systems_with(&[("mskr-1", "MSKR-1", 99, -0.5)]);
        let known: std::collections::HashMap<String, i64> =
            [("zzz-9".to_string(), 42i64), ("mskr-1".to_string(), 7i64)].into_iter().collect();
        let r = analyze("Zzz-9 MSKR-1 tackled", &s, &noships(), &known, 1, "ch", "x");
        assert!(proposed(&r.pilots, "Zzz-9"), "code-shaped name not a pilot: {:?}", r.pilots);
        assert!(!has_pilot_token(&r.pilots, "MSKR-1"), "system code leaked as pilot: {:?}", r.pilots);
        assert!(r.systems.iter().any(|d| d.name == "MSKR-1"), "MSKR-1 not the system: {:?}", r.systems);
    }

    #[test]
    fn pilot_word_that_is_a_hull_is_not_also_a_ship() {
        let s = systems();
        let ships = ships_with(&[("Worm", 17619)]);
        let known: std::collections::HashMap<String, i64> =
            [("bovine worm".to_string(), 1i64)].into_iter().collect();
        let r = analyze("bovine worm", &s, &ships, &known, 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("bovine worm")), "pilots={:?}", r.pilots);
        assert!(!r.ships.iter().any(|sh| sh.name == "Worm"), "Worm inside pilot span leaked: {:?}", r.ships);
        let ctrl = analyze("Bob in a Worm", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(ctrl.ships.iter().any(|sh| sh.name == "Worm"), "control Worm missing: {:?}", ctrl.ships);
    }

    #[test]
    fn noise_punctuation_between_tokens_does_not_break_detection() {
        assert_eq!(tokenize("Slasher, ESS* 346mio"), vec!["Slasher", "ESS", "346mio"]);
        assert_eq!(tokenize("O'Brien I-Pustelga Htg-0"), vec!["O'Brien", "I-Pustelga", "Htg-0"]);
        let s = systems();
        let ships = ships_with(&[("Slasher", 585)]);
        let r = analyze("Slasher*, ESS 60mio", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Slasher"), "ships={:?}", r.ships);
        assert!(r.ess, "ESS flag should fire through the punctuation: {:?}", r.text);
        assert_eq!(r.isk, Some(60_000_000), "isk={:?}", r.isk);
    }

    fn ships_with(names: &[(&str, i64)]) -> std::collections::HashMap<String, (i64, String)> {
        names
            .iter()
            .map(|(n, id)| (n.to_lowercase(), (*id, n.to_string())))
            .collect()
    }

    #[test]
    fn wyf8_kill_list_keeps_all_five_pilots() {
        let s = systems_with(&[("wyf8-8", "WYF8-8", 30002126, -0.4)]);
        let reals =
            ["BoneChilling Chelien", "Gonzilla", "Krombopulous Jaynara", "Rollboy", "ShadowClown-Z"];
        let known: std::collections::HashMap<String, i64> =
            reals.iter().enumerate().map(|(i, r)| (r.to_lowercase(), i as i64 + 1)).collect();
        for line in [
            "BoneChilling Chelien  Gonzilla  Krombopulous Jaynara  Rollboy  ShadowClown-Z  WYF8-8",
            "BoneChilling Chelien Gonzilla Krombopulous Jaynara Rollboy ShadowClown-Z WYF8-8",
        ] {
            let r = analyze(line, &s, &noships(), &known, 1, "ch", "Volltz");
            let (pilots, sysd, _gates) = resolve_report(&r, &reals, &s);
            for name in reals {
                assert!(
                    pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
                    "{line:?}: pilot {name:?} dropped: {pilots:?}",
                );
            }
            assert_eq!(pilots.len(), 5, "{line:?}: expected exactly five pilots: {pilots:?}");
            assert_eq!(sysd, vec!["WYF8-8".to_string()], "{line:?}: system: {sysd:?}");
        }
    }

    #[test]
    fn wyf8_amend_unions_pilots() {
        let s = systems_with(&[("wyf8-8", "WYF8-8", 30002126, -0.4)]);
        let sys = vec![DetectedSystem { id: 30002126, name: "WYF8-8".into(), security: -0.4 }];
        let mut state = IntelState::default();
        state.push(IntelReport {
            pilots: vec!["Krombopulous Jaynara".into(), "Rollboy".into()],
            systems: sys.clone(),
            reporter: "Volltz".into(),
            received: 1,
            text: "Krombopulous Jaynara Rollboy WYF8-8".into(),
            ..Default::default()
        });
        let second = IntelReport {
            pilots: vec!["BoneChilling Chelien".into(), "Gonzilla".into(), "ShadowClown-Z".into()],
            systems: sys,
            reporter: "Volltz".into(),
            received: 10,
            text: "BoneChilling Chelien Gonzilla ShadowClown-Z WYF8-8".into(),
            ..Default::default()
        };
        assert!(state.try_amend(&second, 60, &s), "second WYF8-8 message should amend the first");
        for name in
            ["Krombopulous Jaynara", "Rollboy", "BoneChilling Chelien", "Gonzilla", "ShadowClown-Z"]
        {
            assert!(
                state.reports[0].pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
                "amend dropped {name:?}: {:?}",
                state.reports[0].pilots
            );
        }
    }

    #[test]
    fn reverse_amend_revives_systemless_content() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let ships = ships_with(&[("Rifter", 587), ("Punisher", 597)]);
        let mut state = IntelState::default();

        let orphan = analyze("Rifter Punisher +5", &s, &ships, &noknown(), 100, "intel", "Scout");
        assert!(orphan.systems.is_empty(), "orphan should have no system: {:?}", orphan.systems);
        assert!(!orphan.ships.is_empty(), "orphan should carry ships: {:?}", orphan.ships);
        assert_eq!(orphan.count, Some(5), "orphan count: {:?}", orphan.count);
        state.stash_orphan(orphan, 60, 100);

        let mut sysmsg = analyze("FN0-QS", &s, &ships, &noknown(), 105, "intel", "Scout");
        assert_eq!(state.reverse_amend(&mut sysmsg, 60), 1, "one orphan should merge");
        assert!(sysmsg.systems.iter().any(|d| d.name == "FN0-QS"), "system lost: {:?}", sysmsg.systems);
        for hull in ["Rifter", "Punisher"] {
            assert!(sysmsg.ships.iter().any(|sh| sh.name == hull), "ship {hull} missing: {:?}", sysmsg.ships);
        }
        assert_eq!(sysmsg.count, Some(5), "count not carried: {:?}", sysmsg.count);
        assert!(state.orphans.is_empty(), "orphan buffer not emptied: {:?}", state.orphans.len());
    }

    #[test]
    fn reverse_amend_ors_status_flags() {
        let _s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let mut state = IntelState::default();
        let orphan = IntelReport {
            reporter: "Scout".into(),
            channel: "intel".into(),
            received: 100,
            text: "Loki bubbled cyno nv".into(),
            ships: vec![DetectedShip { id: 29990, name: "Loki".into() }],
            bubble: true,
            cyno: true,
            no_visual: true,
            tackled: true,
            ..Default::default()
        };
        state.stash_orphan(orphan, 60, 100);
        let mut sysmsg = IntelReport {
            reporter: "Scout".into(),
            channel: "intel".into(),
            received: 130,
            text: "FN0-QS".into(),
            systems: vec![DetectedSystem { id: 30004111, name: "FN0-QS".into(), security: -0.4 }],
            ..Default::default()
        };
        assert_eq!(state.reverse_amend(&mut sysmsg, 60), 1);
        assert!(sysmsg.bubble && sysmsg.cyno && sysmsg.no_visual && sysmsg.tackled, "flags not OR-ed");
        assert!(sysmsg.ships.iter().any(|sh| sh.name == "Loki"), "ship missing: {:?}", sysmsg.ships);
    }

    #[test]
    fn reverse_amend_ignores_stale_orphan() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let ships = ships_with(&[("Rifter", 587)]);
        let mut state = IntelState::default();
        let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
        state.stash_orphan(orphan, 60, 100);
        let mut sysmsg = analyze("FN0-QS", &s, &ships, &noknown(), 161, "intel", "Scout");
        assert_eq!(state.reverse_amend(&mut sysmsg, 60), 0, "stale orphan should not merge");
        assert!(sysmsg.ships.is_empty(), "stale ship leaked: {:?}", sysmsg.ships);
        assert!(state.orphans.is_empty(), "stale orphan should be dropped: {:?}", state.orphans.len());
    }

    #[test]
    fn reverse_amend_only_same_reporter_and_channel() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let ships = ships_with(&[("Rifter", 587)]);
        let mut state = IntelState::default();
        let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
        state.stash_orphan(orphan, 60, 100);

        let mut other_rep = analyze("FN0-QS", &s, &ships, &noknown(), 110, "intel", "SomeoneElse");
        assert_eq!(state.reverse_amend(&mut other_rep, 60), 0, "different reporter merged");
        assert!(other_rep.ships.is_empty(), "leaked into other reporter: {:?}", other_rep.ships);
        assert_eq!(state.orphans.len(), 1, "fresh non-matching orphan should be kept");

        let mut other_chan = analyze("FN0-QS", &s, &ships, &noknown(), 115, "other", "Scout");
        assert_eq!(state.reverse_amend(&mut other_chan, 60), 0, "different channel merged");
        assert_eq!(state.orphans.len(), 1, "orphan should still be kept");

        let mut mine = analyze("FN0-QS", &s, &ships, &noknown(), 120, "intel", "Scout");
        assert_eq!(state.reverse_amend(&mut mine, 60), 1, "same reporter/channel should merge");
        assert!(mine.ships.iter().any(|sh| sh.name == "Rifter"), "ship missing: {:?}", mine.ships);
        assert!(state.orphans.is_empty(), "orphan should be consumed");
    }

    #[test]
    fn reverse_amend_skips_clear_report() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let ships = ships_with(&[("Rifter", 587)]);
        let mut state = IntelState::default();
        let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
        state.stash_orphan(orphan, 60, 100);
        let mut clr = analyze("FN0-QS clear", &s, &ships, &noknown(), 110, "intel", "Scout");
        assert!(clr.clear, "should be a clear: {:?}", clr.text);
        assert_eq!(state.reverse_amend(&mut clr, 60), 0, "clear must not reverse-amend");
        assert!(clr.ships.is_empty(), "orphan leaked into clear: {:?}", clr.ships);
        assert_eq!(state.orphans.len(), 1, "orphan should be kept after a clear: {:?}", state.orphans.len());
    }

    #[test]
    fn stash_orphan_prunes_stale() {
        let mut state = IntelState::default();
        let mk = |t: i64| IntelReport {
            reporter: "Scout".into(),
            channel: "intel".into(),
            received: t,
            ships: vec![DetectedShip { id: 587, name: "Rifter".into() }],
            ..Default::default()
        };
        state.stash_orphan(mk(100), 60, 100);
        state.stash_orphan(mk(120), 60, 120);
        state.stash_orphan(mk(200), 60, 200);
        assert_eq!(state.orphans.len(), 1, "only the fresh orphan should remain");
        assert_eq!(state.orphans[0].received, 200);
    }

    #[test]
    fn thera_wormhole_ref_does_not_override_primary() {
        let by_name = [
            ("rancer", "Rancer", 1i64, 0.4),
            ("jita", "Jita", 2, 0.9),
            ("thera", "Thera", 31000005, -1.0),
            ("c-j6mt", "C-J6MT", 5, -0.6),
        ]
        .into_iter()
        .map(|(k, n, id, sec)| {
            (k.to_string(), SystemInfo { id, name: n.to_string(), security: sec, constellation: String::new(), region: String::new(), faction: String::new() })
        })
        .collect();
        let adjacency = [(1i64, vec![2i64]), (2, vec![1])].into_iter().collect();
        let s = Systems::new(by_name, adjacency);
        for (line, primary) in [
            ("Rancer Thera hole", "Rancer"),
            ("Jita Thera hole", "Jita"),
            ("Rancer wh to Thera", "Rancer"),
            ("3 reds Rancer Thera hole", "Rancer"),
            ("C-J Thera hole", "C-J6MT"),
            ("Thera hole Rancer", "Rancer"),
        ] {
            let r = analyze(line, &s, &noships(), &noknown(), 1, "ch", "x");
            let (_p, sysd, gates) = resolve_report(&r, &[], &s);
            assert_eq!(sysd, vec![primary.to_string()], "{line:?}: primary system: {sysd:?}");
            assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Thera")), "{line:?}: Thera became a gate: {gates:?}");
            assert!(matches!(r.wh_dest, Some(crate::wormholes::DestClass::Thera)), "{line:?}: wh_dest: {:?}", r.wh_dest);
        }
        let r = analyze("hostiles in Thera camped", &s, &noships(), &noknown(), 1, "ch", "x");
        let (_p, sysd, _g) = resolve_report(&r, &[], &s);
        assert_eq!(sysd, vec!["Thera".to_string()], "genuine Thera location: {sysd:?}");
    }

    #[test]
    fn system_code_matching_inactive_char_is_the_system() {
        let s = systems_with(&[("ualx", "UALX", 30009003, -0.5), ("dt-", "DT-", 30009004, -0.5)]);
        let known: std::collections::HashMap<String, i64> =
            [("ualx".to_string(), 9i64)].into_iter().collect();
        for denied_ualx in [false, true] {
            let denied: std::collections::HashSet<String> =
                if denied_ualx { ["ualx".to_string()].into() } else { Default::default() };
            let r = analyze_ctx(
                "DT- gate to UALX camped", &s, &noships(), &known, 1, "ch", "x", None, &[], &denied,
            );
            assert!(!has_pilot_token(&r.pilots, "UALX"), "UALX leaked as a pilot: {:?}", r.pilots);
            assert!(r.systems.iter().any(|d| d.name == "UALX"), "UALX not the system: {:?}", r.systems);
            assert!(r.camp, "camp keyword should still fire: {:?}", r.text);
        }
    }

    #[test]
    fn x50em_count_and_ships_after_pilot_all_detected() {
        let s = systems_with(&[("x5-0em", "X5-0EM", 30000777, -0.5)]);
        let ships: std::collections::HashMap<String, (i64, String)> = [
            ("kiki".to_string(), (49711i64, "Kikimora".to_string())),
            ("flycatcher".to_string(), (22464i64, "Flycatcher".to_string())),
            ("kirin".to_string(), (37460i64, "Kirin".to_string())),
        ]
        .into_iter()
        .collect();
        let r = analyze("X5-0EM  dix otto  +12 kikis flycatcher kirin", &s, &ships, &noknown(), 1, "ch", "st0rkant");
        for hull in ["Kikimora", "Flycatcher", "Kirin"] {
            assert!(r.ships.iter().any(|sh| sh.name == hull), "hull {hull} missing: {:?}", r.ships);
        }
        assert_eq!(r.count, Some(13), "count: {:?}", r.count);
        let (pilots, sysd, _g) = resolve_report(&r, &["dix otto"], &s);
        assert_eq!(pilots, vec!["dix otto".to_string()], "pilots: {pilots:?}");
        assert_eq!(sysd, vec!["X5-0EM".to_string()], "system: {sysd:?}");
    }

    #[test]
    fn unresolved_caps_code_in_gate_not_a_pilot() {
        let s = systems();
        let txt = "DT gate to UALX Camped";
        let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "Frizank2");
        let resolved = esi_resolve(&r.pilots, &[]);
        assert!(
            !resolved.iter().any(|p| p == "UALX" || p == "DT"),
            "unresolved system code became a pilot: {resolved:?}",
        );
        assert!(r.camp, "camped should set the camp keyword: {:?}", r.text);
    }

    mod session_regressions {
        use super::*;

        #[test]
        fn line7_repeated_subphrase_pilot_survives() {
            let s = systems_with(&[("hl-vzx", "HL-VZX", 30002, -0.4)]);
            let ships = ships_with(&[("Stabber", 622), ("Orthrus", 33470), ("Stiletto", 11198)]);
            let known: std::collections::HashMap<String, i64> = [
                ("furry for life".to_string(), 1i64),
                ("tiffanbrill".to_string(), 2i64),
                ("tiffanbrill dragon".to_string(), 3i64),
            ]
            .into_iter()
            .collect();
            let line =
                "HL-VZX Furry For Life Tiffanbrill Tiffanbrill Dragon stabber orthrus stiletto";
            let r = analyze(line, &s, &ships, &known, 1, "ch", "Shadow McLane");
            for hull in ["Stabber", "Orthrus", "Stiletto"] {
                assert!(r.ships.iter().any(|sh| sh.name == hull), "missing {hull}: {:?}", r.ships);
            }
            let reals = ["Furry For Life", "Tiffanbrill", "Tiffanbrill Dragon"];
            let (pilots, sysd, _gates) = resolve_report(&r, &reals, &s);
            for name in reals {
                assert!(
                    pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
                    "pilot {name:?} missing: {pilots:?}",
                );
            }
            assert_eq!(sysd, vec!["HL-VZX".to_string()], "system: {sysd:?}");

            let mut prev = analyze(line, &s, &ships, &known, 1, "ch", "Shadow McLane");
            prev.pilots = reals.iter().map(|n| (*n).to_string()).collect();
            let merge_src = format!("{} {}", prev.text, prev.text);
            drop_subphrase_pilots(
                &mut prev.pilots,
                &std::collections::HashSet::new(),
                &merge_src,
            );
            assert!(prev.pilots.iter().any(|p| p == "Tiffanbrill"), "standalone dropped: {:?}", prev.pilots);
            assert!(prev.pilots.iter().any(|p| p == "Tiffanbrill Dragon"), "two-word dropped: {:?}", prev.pilots);
        }

        #[test]
        fn line8_parse_isk_ess_amounts() {
            assert_eq!(parse_isk("ess robbed 30m", true), None, "'robbed' is not an amount");
            assert_eq!(parse_isk("ess 346mio", true), Some(346_000_000));
            assert_eq!(parse_isk("ess 77m", true), Some(77_000_000));
            assert_eq!(parse_isk("ess 300kk 5 min", true), Some(300_000_000));
        }
    }

    #[test]
    fn full_hull_name_detected_in_any_case() {
        let s = systems();
        let ships = ships_with(&[("Rifter", 587), ("Naga", 4306)]);
        for variant in ["RIFTER", "rifter", "RiFtEr", "Rifter"] {
            let r = analyze(variant, &s, &ships, &noknown(), 1, "ch", "x");
            assert!(
                r.ships.iter().any(|sh| sh.name == "Rifter"),
                "{variant:?}: Rifter not detected: {:?}",
                r.ships
            );
        }
        let r = analyze("tackled a NAGA on the gate", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Naga"), "ships={:?}", r.ships);
    }

    #[test]
    fn hulls_after_decorated_count_detected() {
        let s = systems_with(&[("rancer", "Rancer", 1, 0.4)]);
        let ships = ships_with(&[("Vagabond", 11999), ("Cerberus", 11993)]);
        for line in [
            "Rancer  +5 vagabond cerberus",
            "Rancer +5 VAGABOND CERBERUS",
            "Rancer +5 Vagabond Cerberus",
        ] {
            let r = analyze(line, &s, &ships, &noknown(), 1, "ch", "x");
            assert!(r.ships.iter().any(|sh| sh.name == "Vagabond"), "{line:?}: {:?}", r.ships);
            assert!(r.ships.iter().any(|sh| sh.name == "Cerberus"), "{line:?}: {:?}", r.ships);
        }
    }

    #[test]
    fn hull_next_to_confirmed_pilot_still_detected() {
        let s = systems();
        let ships = ships_with(&[("Rifter", 587), ("Sabre", 22456), ("Worm", 17619)]);
        let known: std::collections::HashMap<String, i64> =
            [("bob".to_string(), 1i64), ("wolf e kristjansson".to_string(), 2i64)]
                .into_iter()
                .collect();
        for line in ["Bob Rifter", "Rifter Bob"] {
            let r = analyze(line, &s, &ships, &known, 1, "ch", "x");
            assert!(
                r.ships.iter().any(|sh| sh.name == "Rifter"),
                "{line:?}: hull next to confirmed pilot dropped: {:?}",
                r.ships
            );
            let (pilots, _sys, _g) = resolve_report(&r, &["Bob"], &s);
            assert_eq!(pilots, vec!["Bob".to_string()], "{line:?}: pilot: {pilots:?}");
        }
        let ships2 = ships_with(&[("Wolf", 11371)]);
        let r = analyze("Wolf E Kristjansson nv", &s, &ships2, &known, 1, "ch", "x");
        assert!(r.ships.is_empty(), "confirmed-name hull word leaked: {:?}", r.ships);
        let r = analyze("Sabre Smith in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.is_empty(), "unconfirmed blob forced a ship: {:?}", r.ships);
    }

    #[test]
    fn multiword_hull_names_case_insensitive() {
        let s = systems();
        let ships =
            ships_with(&[("Cyclone Fleet Issue", 17634), ("Naga", 4306), ("Vagabond", 11999)]);
        for line in ["cyclone fleet issue", "CYCLONE FLEET ISSUE", "Cyclone Fleet Issue"] {
            let r = analyze(line, &s, &ships, &noknown(), 1, "ch", "x");
            assert!(
                r.ships.iter().any(|sh| sh.name == "Cyclone Fleet Issue"),
                "{line:?}: {:?}",
                r.ships
            );
        }
        let r = analyze("naga and a CYCLONE FLEET ISSUE", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Naga"), "ships={:?}", r.ships);
        assert!(
            r.ships.iter().any(|sh| sh.name == "Cyclone Fleet Issue"),
            "ships={:?}",
            r.ships
        );
    }

    #[test]
    fn multiword_system_is_the_system_not_a_pilot() {
        let s = systems_with(&[
            ("sanctified vidette", "Sanctified Vidette", 31000123, -1.0),
            ("rancer", "Rancer", 1, 0.4),
        ]);
        let ships = ships_with(&[("Rifter", 587)]);
        for line in ["Sanctified Vidette", "sanctified vidette", "SANCTIFIED VIDETTE"] {
            let r = analyze(line, &s, &ships, &noknown(), 1, "ch", "x");
            assert!(
                r.systems.iter().any(|d| d.name == "Sanctified Vidette"),
                "{line:?}: system missing: {:?}",
                r.systems
            );
            assert!(
                !r.pilots.iter().any(|p| p.to_lowercase().contains("vidette")
                    || p.to_lowercase().contains("sanctified")),
                "{line:?}: leaked as pilot: {:?}",
                r.pilots
            );
        }
        let known: std::collections::HashMap<String, i64> =
            [("bob".to_string(), 1i64)].into_iter().collect();
        let r = analyze("Bob Rifter Sanctified Vidette", &s, &ships, &known, 1, "ch", "x");
        assert!(
            r.systems.iter().any(|d| d.name == "Sanctified Vidette"),
            "system missing: {:?}",
            r.systems
        );
        assert!(r.ships.iter().any(|sh| sh.name == "Rifter"), "ship missing: {:?}", r.ships);
        let (pilots, _sys, _g) = resolve_report(&r, &["Bob"], &s);
        assert_eq!(pilots, vec!["Bob".to_string()], "pilots: {pilots:?}");

        let known2: std::collections::HashMap<String, i64> =
            [("john smith".to_string(), 7i64)].into_iter().collect();
        let r = analyze("John Smith in Rancer", &s, &noships(), &known2, 1, "ch", "x");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("John Smith")),
            "normal 2-word pilot dropped: {:?}",
            r.pilots
        );
        assert!(r.systems.iter().any(|d| d.name == "Rancer"), "systems={:?}", r.systems);
    }

    #[test]
    fn paste_segment_trailing_tag_stripped_ben_walker() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let known: std::collections::HashMap<String, i64> =
            [("ben walker".to_string(), 1i64)].into_iter().collect();
        for line in ["FN0-QS  Ben Walker NV", "FN0-QS Ben Walker NV", "FN0-QS  Ben Walker  NV"] {
            let r = analyze(line, &s, &noships(), &known, 1, "ch", "x");
            assert!(
                r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ben Walker")),
                "{line:?}: Ben Walker not recognized: {:?}",
                r.pilots
            );
            assert!(
                !r.pilots.iter().any(|p| p.to_lowercase().contains("nv")),
                "{line:?}: NV glued into a pilot: {:?}",
                r.pilots
            );
            assert!(r.no_visual, "{line:?}: no_visual not set: {:?}", r.text);
            assert!(
                r.systems.iter().any(|d| d.name == "FN0-QS"),
                "{line:?}: system missing: {:?}",
                r.systems
            );
            let (pilots, _sys, _g) = resolve_report(&r, &["Ben Walker"], &s);
            assert!(pilots.iter().any(|p| p == "Ben Walker"), "{line:?}: cover: {pilots:?}");
        }
        assert_eq!(trim_paste_location_tail("Ben Walker NV", &ships_with(&[])), "Ben Walker");
        assert_eq!(trim_paste_location_tail("Ben Walker nv", &ships_with(&[])), "Ben Walker");
        assert_eq!(trim_paste_location_tail("Clear Rain", &ships_with(&[])), "Clear Rain");
        assert_eq!(trim_paste_location_tail("Blue Skies", &ships_with(&[])), "Blue Skies");
        assert_eq!(trim_paste_location_tail("Lopatich R", &ships_with(&[])), "Lopatich R");
        assert_eq!(trim_paste_location_tail("Malcolm 41", &ships_with(&[])), "Malcolm 41");
    }
}
