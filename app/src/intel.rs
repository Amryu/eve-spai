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

/// An ambiguous ship abbreviation (e.g. "SFI") and the hulls it could mean. Shown as an
/// informational badge; `candidates` carry the resolved type id (0 if the SDE lacks it) for icons.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AmbiguousShip {
    pub abbrev: String,
    pub candidates: Vec<(i64, String)>,
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
    /// Ambiguous ship abbreviations seen in the text (e.g. "SFI" = Scythe/Stabber Fleet Issue).
    /// Shown as an informational badge listing the candidates; cleared when an amending message
    /// names the full ship. See `crate::shipnames::ambiguous_candidates`.
    #[serde(default)]
    pub ambiguous_ships: Vec<AmbiguousShip>,
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
    seen_lines: std::collections::HashSet<u64>,
    seen_order: std::collections::VecDeque<u64>,
}

static NEXT_REPORT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl IntelState {
    /// True when this exact log line was already ingested. Multiple accounts (and relog/rejoin
    /// files) carry identical `(channel, timestamp, reporter, text)` lines; the raw timestamp
    /// string is used so a parse failure can't split a duplicate into two keys.
    pub fn duplicate_line(&mut self, channel: &str, ts: &str, reporter: &str, text: &str) -> bool {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        channel.hash(&mut h);
        ts.hash(&mut h);
        reporter.hash(&mut h);
        text.hash(&mut h);
        let key = h.finish();
        if !self.seen_lines.insert(key) {
            return true;
        }
        self.seen_order.push_back(key);
        if self.seen_order.len() > 8192 {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen_lines.remove(&old);
            }
        }
        false
    }

    pub fn push(&mut self, mut report: IntelReport) -> u64 {
        let id = NEXT_REPORT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        report.id = id;
        // A "clear" means the hostiles aren't there *now*, so earlier sightings are
        // greyed as outdated, not erased.
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
        // A clear is always its own report, so it never overwrites the threat info of a
        // prior sighting.
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
            merge_ambiguous_ships(&mut prev.ambiguous_ships, &new.ambiguous_ships, &prev.ships);
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
    merge_ambiguous_ships(&mut dst.ambiguous_ships, &src.ambiguous_ships, &dst.ships);
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
        || crate::shipnames::ambiguous_candidates(&lw).is_some()
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

/// Victim pilot and ship from a pasted killmail-link display string, `<killword><colon> <Victim>
/// (<Ship>)`, e.g. "Kill: Lord Road (Loki)". The chat log strips the `<url=killReport...>` tag, and
/// in some locales the killword and colon glue to the first name word ("击杀：Lord Road" is one
/// whitespace token), so the victim never forms via the normal paths.
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

/// True when `bare` appears in `source` as "bare'" (trailing apostrophe) immediately followed by
/// another name-part word, i.e. it is the first half of an apostrophe name like "Jennifer' Thyron".
/// tokenize() strips the apostrophe, so without this the bare prefix leaks as a separate pilot while
/// the full two-word name is also detected.
fn apostrophe_name_prefix(source: &str, bare: &str) -> bool {
    let words: Vec<&str> = source.split_whitespace().collect();
    let want = format!("{bare}'");
    for (i, w) in words.iter().enumerate() {
        if w.eq_ignore_ascii_case(&want) {
            if let Some(next) = words.get(i + 1) {
                let core = next.trim_matches(|c: char| ",.;:!?\"()".contains(c));
                if name_part(core) {
                    return true;
                }
            }
        }
    }
    false
}

/// Union `src` ambiguous abbreviations into `dst`, then drop any that a now-present ship resolves
/// (a later message naming the full hull removes the badge).
fn merge_ambiguous_ships(dst: &mut Vec<AmbiguousShip>, src: &[AmbiguousShip], ships: &[DetectedShip]) {
    for a in src {
        if !dst.iter().any(|x| x.abbrev.eq_ignore_ascii_case(&a.abbrev)) {
            dst.push(a.clone());
        }
    }
    dst.retain(|a| !a.candidates.iter().any(|(_, name)| ships.iter().any(|s| s.name.eq_ignore_ascii_case(name))));
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

    pub fn revived(&self, name: &str, now: i64) -> bool {
        self.distinct_systems_since(name, 3600, now) >= 3
            || self.distinct_systems_since(name, SIGHTINGS_WINDOW, now) >= 5
    }
}

#[cfg(test)]
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
            let dest_lc = dest.name.to_lowercase();
            gate = Some(dest.name.clone());
            // An ansiblex is a player stargate to a neighbouring system: consume the keyword and
            // the token naming the destination ("EFM" in "EFM Ansi") so neither becomes a pilot.
            for tok in tokens.iter() {
                let lc = tok.to_lowercase();
                if consumed.contains(&lc) {
                    continue;
                }
                let names_dest = lc.len() >= 3
                    && dest_lc.starts_with(&lc)
                    && tok.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
                if lc == "ansi" || lc == "ansiblex" || names_dest {
                    consumed.push(lc);
                }
            }
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
            && !apostrophe_name_prefix(text, t)
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
            && !apostrophe_name_prefix(text, t)
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
    // two candidate spans partially overlap ("Lord Road" vs "Road he's"), keep the stronger name
    // (known, else longer, else leftmost) so a tail a longer name already consumed can't seed a
    // bogus second pilot. Positions decide this, never letter case.
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
    pilots.retain(|p| {
        let content: Vec<&str> = p.split_whitespace().filter(|w| !is_pilot_stopword(w)).collect();
        content.len() < 4
            || quoted.contains(&p.to_lowercase())
            || paste_origin.contains(&p.to_lowercase())
            || !content.iter().all(|w| crate::dict::is_word(w))
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
    // Ambiguous ship abbreviations (e.g. "SFI"): surface as a badge unless a candidate hull is
    // already named in this same message.
    let ambiguous_ships: Vec<AmbiguousShip> = {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<AmbiguousShip> = Vec::new();
        for t in &tokens {
            let Some(cands) = crate::shipnames::ambiguous_candidates(t) else { continue };
            if !seen.insert(t.to_lowercase()) {
                continue;
            }
            let resolved_here =
                cands.iter().any(|c| ships.iter().any(|s| s.name.eq_ignore_ascii_case(c)));
            if resolved_here {
                continue;
            }
            let candidates = cands
                .iter()
                .map(|c| match ship_of(&c.to_lowercase(), ship_index) {
                    Some((id, name)) => (*id, name.clone()),
                    None => (0, (*c).to_owned()),
                })
                .collect();
            out.push(AmbiguousShip { abbrev: t.to_uppercase(), candidates });
        }
        out
    };
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
        ambiguous_ships,
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
        // Exact match, not prefix: the "drag" stem would match the destroyer Dragoon.
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

/// An approximate ISK amount posted in intel ("300kk", "1.5b", "300 mil", "300 million"),
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
            // In ESS context an amount below 50M is almost always a time ("30m" = 30 minutes),
            // since ESS banks worth calling out are >= 50M. Drop the small ones so they don't
            // double-parse as an ISK amount alongside the hack timer.
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

/// Derive the hostile count from its components and the current pilot count. `named` is the number
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

/// A bare number the name parser already swallowed, returned as the "Lead N" candidate it belongs
/// to. The pair has to appear in a name that was actually detected, which is what separates
/// "Trinity 5 red" (pilot "Trinity 5", so the 5 is part of a name) from "ESS 5 reds" (no pilot
/// "ESS 5", so the 5 is a count) even though both put a capitalised word in front of the number.
///
/// The caller records it in `name_number_skips` rather than dropping it: if resolution decides the
/// candidate is not a real character, the alert engine adds the number back as a ship count.
fn number_in_pilot_name(
    words: &[&str],
    i: usize,
    digits: &str,
    pilots: &[String],
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
) -> Option<String> {
    let lead = words[i.checked_sub(1)?]
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-');
    if !name_part(lead)
        || systems.lookup(lead).is_some()
        || ship_index.contains_key(&lead.to_ascii_lowercase())
    {
        return None;
    }
    let pair = format!("{lead} {digits}");
    let needle = pair.to_ascii_lowercase();
    pilots.iter().any(|p| p.to_ascii_lowercase().contains(&needle)).then_some(pair)
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
    // A number in front of one of these is a quantity of something else, and every one of them is
    // separately parsed elsewhere: ISK magnitudes by `parse_isk`, durations by `parse_time_left`
    // (the ESS hack timer), distances by nothing at all. A number claimed by any of them must not
    // also be read as a hostile count, however it is qualified: "ess reds 5 min" is a 5 minute
    // timer, not 5 reds, and "reds 20 km off gate" is a range.
    const UNIT_WORDS: &[&str] = &[
        "m", "mil", "mill", "million", "millions", "mio", "kk", "b", "bil", "bill", "billion",
        "billions", "k", "isk", "min", "mins", "minute", "minutes", "s", "sec", "secs", "second",
        "seconds", "h", "hr", "hrs", "hour", "hours", "km", "kms", "au",
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
        // Before the qualification test, not after it: a number inside a name stays out of the
        // count even when a red/neut keyword or a ship sits on its other side.
        if bare_number {
            if let Some(cand) = number_in_pilot_name(&words, i, digits, pilots, systems, ship_index)
            {
                if let Ok(n) = digits.parse::<u32>() {
                    name_skips.push((cand, n));
                }
                continue;
            }
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
                if UNIT_WORDS.contains(&n.as_str()) {
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
mod tests;
