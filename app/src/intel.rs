//! Intel parsing and decay state (docs/DESIGN.md §7.1 E3/E4).
//!
//! Parses a chat message into a concise, structured report: detected solar systems
//! (matched against the SDE), an approximate hostile count, and status flags
//! (clear / no-visual / spike / gate camp / bubble / killmail). The raw text is
//! kept but de-emphasised in the UI.

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

/// Scanning-probe badge kind. A small enum (rather than the `&'static str` it used to be) so the
/// report can derive serde for the overlay IPC without dragging a `'de: 'static` bound onto every
/// containing type; [`Probes::label`] / its `Display` give the badge text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Probes {
    /// Core Scanner Probes (cosmic signatures).
    Core,
    /// Combat Scanner Probes (ships / structures).
    Combat,
    /// Unspecified scanner probes.
    Any,
}

impl Probes {
    /// The badge label shown in the UI.
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

/// Whether an `anom`/`sig` callout named a combat anomaly or a cosmic signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AnomKind {
    Anomaly,
    Signature,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct IntelReport {
    /// Stable per-report id (assigned on push, preserved across amendments) so the alert
    /// window can re-find a report after its content — and thus report_key — changes.
    pub id: u64,
    /// Unix seconds (from the message's EVE timestamp when parseable).
    pub received: i64,
    pub channel: String,
    pub reporter: String,
    pub text: String,
    pub systems: Vec<DetectedSystem>,
    pub ships: Vec<DetectedShip>,
    /// Ship classes named only by keyword, no specific hull ("dic", "recon", "logi").
    pub classes: Vec<String>,
    /// Candidate pilot names (Title-Case word runs); confirmed by ESI later.
    pub pilots: Vec<String>,
    /// Approximate hostile/ship count parsed from the message, if any.
    pub count: Option<u32>,
    /// Bare numbers tentatively treated as name components ("Adama 80"): (candidate, n).
    /// The reconcile adds `n` back to the count if ESI says the candidate isn't a pilot.
    pub name_number_skips: Vec<(String, u32)>,
    /// An ISK amount posted in the message ("300kk" -> 300_000_000), if any.
    pub isk: Option<u64>,
    /// Structures mentioned (canonical name) + an optional distance off each.
    pub structures: Vec<(String, Option<String>)>,
    /// Celestial locations named in the message ("Planet 4", "Moon 3", "Sun").
    pub celestials: Vec<String>,
    /// Scanning probes mentioned (Core/Combat Scanner Probe items + slang) — distinct from
    /// the Probe frigate. The badge label ("Core Probes"/"Combat Probes"/"Probes"), or None.
    pub probes: Option<Probes>,
    pub clear: bool,
    /// Someone explicitly asking for intel ("status?") — informational, not a threat.
    pub status: bool,
    pub no_visual: bool,
    pub spike: bool,
    pub camp: bool,
    /// A call for help / backup (help / sos / "need backup").
    pub help: bool,
    pub bubble: bool,
    pub killmail: bool,
    /// Killmail only: the nearest celestial to the death (name, distance in metres), when the kill
    /// carried an in-space position. The card shows it when the distance is within ~15,000 km.
    #[serde(default)]
    pub near_celestial: Option<(String, f64)>,
    pub cyno: bool,
    /// A hot-drop / black-ops threat ("dropper", "hotdropper", "blops", …).
    pub dropper: bool,
    /// A capital ship (cap / rorqual / dread / carrier / …) reported tackled.
    pub cap_tackled: bool,
    /// A (non-capital) ship/type reported tackled.
    pub tackled: bool,
    /// Ship/class names reported tackled ("Loki", "Marauder"), for the TACKLED badge.
    pub tackled_targets: Vec<String>,
    pub wormhole: bool,
    /// Detected wormhole signature code (e.g. "K162"), when one was named.
    pub wh_type: Option<String>,
    /// Destination class guessed from the message text. This is only a guess — the
    /// wormhole *type's* own class (and EVE-Scout data) are authoritative facts and
    /// override it; it's used only when the type doesn't pin the class.
    pub wh_dest: Option<crate::wormholes::DestClass>,
    /// "EOL" / end-of-life was called out.
    pub wh_eol: bool,
    /// "drifter" was called out.
    pub wh_drifter: bool,
    /// Cosmic signature id (e.g. "ABC-123"), if named.
    pub wh_sig: Option<String>,
    pub ess: bool,
    /// Time left until the ESS is hacked, when called out (e.g. "5:30", "3m").
    pub ess_time: Option<String>,
    pub skyhook: bool,
    /// A filament / needlejack / trace was called out — a roaming gang jumping in.
    pub filament: bool,
    /// "Diamond rats" (the dangerous NPC pirate variant) were called out — NPCs, not pilots.
    pub diamond_rats: bool,
    /// Anomaly/signature ids called out next to an "anom"/"sig" keyword (kind + code, e.g.
    /// `(Anomaly, "ABC-123")`). Each renders a badge ("Anom ABC-123" / "Sig ABC-123").
    pub anom_sigs: Vec<(AnomKind, String)>,
    /// Gate the hostiles are reported on, e.g. "78-" in "C-J +20 on 78- gate".
    /// Gates mentioned in the report (a card has one system but may name several
    /// gates — extra system mentions are demoted to gates).
    pub gates: Vec<String>,
    /// Alliances mentioned by shorthand → (name, alliance id) for the logo.
    pub alliances: Vec<(String, i64)>,
    /// Where the subject was previously seen (set by the watcher).
    pub movement: Option<Movement>,
    /// External links pasted into the message (killmail / battle report / dscan).
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
    /// zKillboard kill id, when this is a killmail link (for dedup with the feed).
    pub kill_id: Option<i64>,
}

/// Blank every pasted http(s) URL token (replace its characters with spaces, preserving spacing so
/// double-space paste delimiters survive) so the parser never reads a link's host/path/hash
/// fragments as pilots, ships, or systems. Links are captured by [`extract_links`] beforehand.
fn strip_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for tok in text.split_inclusive(char::is_whitespace) {
        let word = tok.trim_end_matches(char::is_whitespace);
        let bare = word.trim_start_matches(|c: char| "<>()[]\"'".contains(c));
        if bare.starts_with("http://") || bare.starts_with("https://") {
            out.extend(word.chars().map(|_| ' '));
            out.push_str(&tok[word.len()..]); // keep the trailing whitespace
        } else {
            out.push_str(tok);
        }
    }
    out
}

/// Find pasted killmail / battle-report / dscan URLs in a message.
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
            // Battle reports: br.evetools.org, zKill "related", and OUR own site (eve-spai.com/br/<id>).
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
    /// The first detected system (the report's primary location).
    pub fn primary_system(&self) -> Option<&DetectedSystem> {
        self.systems.first()
    }
}

#[derive(Default)]
pub struct IntelState {
    pub reports: Vec<IntelReport>,
    /// Most recent "clear" time per system (lower-cased name -> unix seconds).
    cleared: HashMap<String, i64>,
    /// Content-bearing reports (pilots/ships) that were discarded for lacking a system,
    /// buffered so a later system message from the same reporter can revive them
    /// ("reverse amend" — see [`stash_orphan`](IntelState::stash_orphan) /
    /// [`reverse_amend`](IntelState::reverse_amend)). Bounded by the amend grace window.
    orphans: Vec<IntelReport>,
}

static NEXT_REPORT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl IntelState {
    /// Push a report and return its assigned stable id.
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

    /// Amend the reporter's most recent report (within `grace` seconds) instead of
    /// adding a new one — for intel split across successive messages. Only when the
    /// new message mentions the **same system or no system** (a gate is not a
    /// system) and actually adds something. Returns true if it amended.
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
            || new.cap_tackled;
        if !adds {
            return false;
        }
        let new_sys = new.primary_system().map(|s| s.id);
        let new_pilots: std::collections::HashSet<String> =
            new.pilots.iter().map(|p| p.to_lowercase()).collect();
        // Individual pilot *words*, minus any held system token — so two reports of the same pilot
        // whose name still holds a system link by the actual name word even when the unresolved
        // blobs differ ("G-EURJ Keeves" and "Keeves G-EURJ" both share "keeves", not "g-eurj").
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
            // Link by the same reporter (split message) OR a shared pilot name (one
            // scout reports the hostile, another adds the ship/route on the same
            // pilot — not linked by system, but by player).
            let same_reporter = prev.reporter == new.reporter;
            let shares_pilot = (!new_pilots.is_empty()
                && prev.pilots.iter().any(|p| new_pilots.contains(&p.to_lowercase())))
                || (!new_words.is_empty()
                    && name_words(&prev.pilots).intersection(&new_words).next().is_some());
            if !same_reporter && !shares_pilot {
                continue;
            }
            // Only amend within the grace window (keep scanning older ones otherwise).
            if new.received < prev.received || new.received - prev.received > grace {
                continue;
            }
            let prev_sys = prev.primary_system().map(|s| s.id);
            // A different *named* system is a new sighting / movement. But a follow-up that adds
            // a system the prior sighting lacked (prev had none) just backfills the location of
            // the same pilot — allow it, mirroring the prev-has-system / new-has-none merge.
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
            // Sub-phrase dedup over the merged pilot list (no protected set: all names are
            // ESI-confirmed candidates, none authoritative). Both reports' raw text is the
            // occurrence source, so a name repeated as a standalone AND inside a longer name across
            // the two messages ("Tiffanbrill" + "Tiffanbrill Dragon") survives the dedup.
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
            prev.count = match (prev.count, new.count) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
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
            prev.received = new.received; // refresh so it re-alerts and reads as fresh
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

    /// Buffer a content-bearing report that was discarded for lacking a system, so a later
    /// system message from the SAME reporter can revive it ([`reverse_amend`](Self::reverse_amend)).
    /// Prunes orphans older than `grace` first (so the buffer stays bounded by recent message
    /// flow), then stashes `report`. The caller only stashes reports that actually carry
    /// content (pilots or ships) — pure chatter is dropped, not buffered.
    pub fn stash_orphan(&mut self, report: IntelReport, grace: i64, now: i64) {
        self.orphans.retain(|o| now - o.received <= grace);
        self.orphans.push(report);
    }

    /// Reverse amend: fold this reporter's very recent system-less reports (buffered by
    /// [`stash_orphan`](Self::stash_orphan)) INTO `new`, a fresh report that carries a system.
    /// This recovers intel that was split "content first, system second" ("Rifter Punisher +5"
    /// then "FN0-QS") — the forward [`try_amend`](Self::try_amend) can't, since the system-less
    /// message was never pushed.
    ///
    /// Only orphans from the same reporter AND channel, no older than `grace` seconds, are
    /// merged (unioning pilots/ships/classes and every other content field, OR-ing status
    /// flags — mirroring `try_amend`'s field list). Consumed orphans are removed; still-fresh
    /// non-matching orphans are kept for a later system; stale ones are dropped. A `clear` or
    /// system-less `new` merges nothing (but still prunes stale orphans). Returns how many
    /// orphans were merged.
    pub fn reverse_amend(&mut self, new: &mut IntelReport, grace: i64) -> usize {
        // A clear stands alone (it must not absorb prior threats), and a report with no system
        // has nothing to give the orphans — nothing to merge. Still prune stale orphans so the
        // buffer can't grow unbounded.
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
                // A still-fresh orphan from another reporter/channel waits for its own system.
                kept.push(o);
            }
            // A stale orphan (from anyone) is dropped.
        }
        self.orphans = kept;
        merged
    }

    /// A non-clear sighting is stale if a clear for one of its systems arrived at
    /// or after it — the hostiles have since left.
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

/// Fold every content field of `src` into `dst`: union collections (dedup as `try_amend` does),
/// OR each status/threat flag, and fill an empty option (max when both are set). Used by
/// [`IntelState::reverse_amend`], with `dst` (the report carrying the system) as the survivor.
/// `clear` and `systems` are intentionally NOT touched — the survivor keeps its own location and
/// a reverse amend never happens on a clear.
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
    // Sub-phrase dedup over the merged pilot list, using both messages' raw text as the
    // occurrence source (mirrors `try_amend`).
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
    dst.count = match (dst.count, src.count) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
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
    dst.wh_eol |= src.wh_eol;
    dst.wh_drifter |= src.wh_drifter;
    dst.wh_sig = dst.wh_sig.clone().or_else(|| src.wh_sig.clone());
    dst.ess |= src.ess;
    dst.ess_time = dst.ess_time.clone().or_else(|| src.ess_time.clone());
    dst.skyhook |= src.skyhook;
    // Show the revived content on the card, oldest first (orphan, then the system message).
    dst.text = format!("{}  ·  {}", src.text, dst.text);
}

const CLEAR_WORDS: &[&str] = &["clear", "clr", "cleared", "clr+", "safe"];

/// Active pilots whose names embed intel keywords. Matched case-sensitively against the readable
/// text so the keyword inside the name is never read as a status keyword. Extend as needed.
const KEYWORD_NAME_PILOTS: &[&str] = &["Clean cyno toon", "RSS Scanner Probe", "clear rain"];

/// Common Title-Case intel/English words that are not pilot names.
const PILOT_STOP: &[&str] = &[
    "gate", "camp", "gatecamp", "gatecamps", "clear", "clr", "spike", "bubble", "drag", "dragbubble", "cyno", "local", "dock", "docked",
    "station", "kill", "killmail", "dead", "ded", "pod", "no", "visual", "nv", "ess", "skyhook", "hostile",
    "filament", "filaments", "needlejack", "needlejacks", "trace", "traces",
    "hostiles", "neut", "neutral", "neuts", "red", "reds", "blue", "blues", "gang", "fleet",
    "bridge", "jump", "jumping", "warp", "warping", "the", "incoming", "inc", "coming", "gcc",
    "afk", "warpin", "system", "and", "for", "status", "stat", "eyes", "any", "report", "intel", "went", "going",
    "help", "sos", "backup", "need",
    // Location/address prose ("hostiles in space", "you guys", "in Jita") — never pilots.
    "guys", "in", "space",
    // Common English filler words that are never pilot names (kept conservative so we
    // don't drop real character names).
    "just", "is", "are", "was", "were", "be", "been", "has", "have", "had", "not", "but",
    "now", "still", "back", "with", "this", "that", "they", "them", "their", "here", "there", "to",
    "crit", "wrong", "channel", "see", "nothing", "else", "safe",
    // Common English words that are ALSO real cached pilot names (found by scanning the known-pilot
    // cache against an English dictionary). Stop-listed so the cache doesn't false-match them when
    // they appear as plain prose — a player happening to be named "Time"/"Worm" loses to the word.
    "about", "after", "because", "call", "came", "can", "come", "could", "day", "did", "die",
    "even", "feel", "find", "first", "form", "get", "give", "good", "her", "his", "its", "keep",
    "know", "leave", "let", "like", "look", "lose", "love", "make", "mean", "most", "new", "our",
    "pay", "people", "put", "read", "run", "said", "saw", "send", "set", "she", "show", "stand",
    "start", "stay", "take", "talk", "tell", "than", "then", "these", "time", "took", "try", "two",
    "understand", "use", "want", "watch", "will", "win", "work", "worm",
    "from", "got", "off", "out", "near", "into", "onto", "over", "your", "youre", "again",
    // "rest" as in "1 jackdaw, rest NV" — never a pilot, even though a character is named "Rest".
    "rest", "stop",
    // Activities / events, never pilots ("ess hacking", "ratting", "missing ships", "I guess").
    // Note: the pronoun "I" needs no entry — a 1-letter token already fails `name_part`
    // (len >= 2), and listing "i" could split a real name that contains a standalone "I".
    "hacking", "hack", "hacked", "ratting", "ratted", "missing", "guess",
    // Hedging verbs ("i think", "i thought", "i believe", "maybe", "probably") — the
    // known-pilot cache otherwise matches real players named "Think"/"Believe" here.
    "think", "thought", "believe", "maybe", "probably", "prob", "probs",
    // Status words ("system clean", "reported in X", "nothing yet").
    "clean", "reported", "yet",
    // Fitting talk ("50mn fit", "shield fit") — a prop-mod size or "fit" is never a pilot.
    "50mn", "fit",
    // Positional / quantity filler ("on grid", "off grid", "a few", "possible hostile", "clear atm").
    "on", "grid", "ongrid", "offgrid", "few", "possible", "atm", "many", "outside", "entrance",
    "linked", "side", // "other side of the gate" — "side" is never a pilot.
    // Structure-grief verbs ("skyhook theft", "poco bash") — keywords, never pilots.
    "theft", "stealing", "stole", "bash", "bashing", "reinforced", "reinforce", "rf",
    // Hot-drop / black-ops threat words — keywords, never pilots ("hot dropper", "blops").
    "drop", "dropper", "droppers", "hotdrop", "hotdrops", "hotdropper", "hotdroppers",
    "hotdropping", "blops", "blackops", "blackop",
    // Engagement descriptors ("good fight", "engaged on gate"). "combat" is covered above.
    "fight", "fights", "fighting", "engaged", "engage", "engaging",
    // "etc" / "etc." (the trailing dot is trimmed by the tokenizer).
    "etc",
    // "more" as in "5 more inbound".
    "more",
    // "scan" / "dscan" ("vedmak on scan", "nothing on dscan"); "scanner" is covered above.
    "scan", "scans", "dscan", "scanning",
    // "drifter" / "drifters" — a wormhole type ("drifter wh"), never a pilot.
    "drifter", "drifters",
    // Pronouns / filler.
    "him", "other", "only", "unless", "end", "also", "confirm", "confirmed", "clearing",
    // Enemy/location prose ("an enemy roaming somewhere", "mostly around the gate").
    "enemies", "enemy", "mostly", "around", "an", "roaming", "somewhere", "support",
    // Filler / hedging words ("unsure which", "too many", "kitchen sink", "catch all").
    "unsure", "which", "too", "kitchen", "sink", "catch", "all",
    // Question / filler words — lower-cased English the known-pilot cache otherwise matches
    // against real players named like common words.
    "what", "where", "when", "who", "why", "how", "well", "anyway", "huh", "hmm", "hmmm",
    "wait", "sure", "dunno", "yes", "yeah", "yep", "yup", "nope", "nah", "ok", "okay", "kk",
    // Chat abbreviations / reactions.
    "sry", "sorry", "ty", "tyvm", "thx", "thanks", "thanx", "np", "yw", "cheers", "lol",
    "lmao", "rofl", "omg", "omw", "wtf", "wth", "ffs", "gg", "wp", "ez", "gj", "gz", "grats",
    "imo", "tbh", "idk", "ikr", "btw", "fyi", "pls", "plz", "plox", "brb", "gtg", "glhf",
    "gl", "hf", "cya", "ttyl", "sup", "yo", "o7", "07", "rip",
    // Common English contractions — never pilot names ("I'm tackled", "they're warping in").
    // BOTH the apostrophe and apostrophe-stripped forms are listed (the tokenizer keeps an
    // internal apostrophe, e.g. "I'm", but a stray one may be trimmed); `is_pilot_stopword`
    // lowercases. Kept to safe contractions whose stripped form is not a plausible single name.
    "im", "i'm", "youre", "you're", "theyre", "they're", "we're",
    "its", "it's", "dont", "don't", "cant", "can't", "wont", "won't",
    "thats", "that's", "whats", "what's", "lets", "let's", "gonna", "wanna",
];

/// Intel keywords that name a ship *class* (not a specific hull) -> canonical class.
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
    // Generic hull sizes ("CRUISERS", "battleships") — a ship type, never a pilot name.
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

/// Ship classes named by keyword in the message (exact token match, in order, deduped).
fn detect_classes(lower_tokens: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in lower_tokens {
        if let Some((_, class)) = SHIP_CLASSES.iter().find(|(k, _)| *k == t.as_str()) {
            if !out.iter().any(|c| c == class) {
                out.push((*class).to_owned());
            }
        }
    }
    out
}

/// Whether a (sub-)name is a stop / ship-descriptor word that should never be accepted
/// as a pilot even if some character happens to share it (used to filter resolver
/// sub-span covers). Conservative so real names aren't dropped.
pub fn is_pilot_stopword(w: &str) -> bool {
    let lw = w.to_lowercase();
    // A MULTI-WORD candidate made up ENTIRELY of stop words ("they are", "back to", "still here")
    // is English prose, never a pilot — reject it as a whole. A single non-stop word anywhere
    // ("Navy Pilot Bob") keeps the candidate. Single-word callers fall through unchanged (each
    // recursion below hits the single-word path).
    if lw.split_whitespace().nth(1).is_some() {
        return lw.split_whitespace().all(is_pilot_stopword);
    }
    PILOT_STOP.contains(&lw.as_str())
        // Any ship-class keyword ("cruisers", "logi", "dic", …) is a ship type, not a pilot.
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
                // Anomaly/signature keywords are a badge (with an adjacent code) or noise on
                // their own — never a pilot. NPC "rats" (incl. the "diamond" variant) are NPCs.
                | "anom" | "anomaly" | "anomalies" | "signature" | "signatures"
                | "rat" | "rats" | "diamond" | "dia"
                // Scanner probes are a badge, never a pilot ("Combat Probes", "Core Scanner
                // Probe", "combat prob"). The Probe frigate is still detected via the ship index.
                | "probe" | "probes" | "prob" | "probs" | "combat" | "core" | "scanner" | "sisters"
                // Alliance ticker (EVE University), not a player — even though a character
                // happens to be named "ivy".
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
        )
}

/// Whether a candidate word is "lowercase" for the single-word dictionary prose filter: it has NO
/// ASCII uppercase letter, OR it is exactly the pronoun "I" (an upper-case single letter that is
/// prose, not a name). A candidate with any OTHER uppercase letter is treated as a name and is never
/// dropped by the dictionary — names win. Only used on single-word candidates.
pub fn is_lowercaseish(w: &str) -> bool {
    w == "I" || !w.chars().any(|c| c.is_ascii_uppercase())
}

/// Quoted spans (delimited by `"`, `'` or `` ` ``, openings/closings may be mixed)
/// — forced to be treated as pilot names rather than keywords/systems. A quote only
/// opens at a word boundary so apostrophes inside names (e.g. "O'Brien") are safe.
fn extract_quoted(text: &str) -> Vec<String> {
    let is_quote = |c: char| c == '"' || c == '\'' || c == '`';
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if is_quote(chars[i]) && (i == 0 || chars[i - 1].is_whitespace()) {
            // Find a closing quote at a word boundary (followed by space/punct/end).
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

/// Candidate pilot names: runs of 2–3 Title-Case alphabetic words in the *raw*
/// text (punctuation and numbers break a run), minus obvious intel/English words.
/// ESI confirms which are real characters later.
/// A token that can be part of an EVE character name. Names can contain digits
/// ("Pericle No1"), apostrophes, and hyphens ("I-Pustelga"). A hyphenated token
/// must have a lower-case letter, else it's an all-caps system code ("67Y-NR").
fn name_part(t: &str) -> bool {
    t.len() >= 2
        && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
        && t.chars().any(|c| c.is_ascii_alphabetic())
        && (!t.contains('-') || t.chars().any(|c| c.is_ascii_lowercase()))
}

/// Nullsec system codes / abbreviations ("C-J", "88A-RA", "1DH-SX"): short alphanumerics
/// joined by a hyphen. This is a CASE-INSENSITIVE *pattern hint* only — "Htg-0", "htg-0"
/// and "HTG-0" all match the code shape — so it must never be the sole gate that keeps a
/// token out of pilot detection. The authoritative "is this really a system" decision is a
/// real systems lookup at the call sites (`is_system_token` / `is_code_lookalike_name`):
/// a token that matches this shape but resolves to no real system is fair game for a pilot.
fn looks_like_system_code(t: &str) -> bool {
    if t.len() < 2 || !t.contains('-') {
        return false;
    }
    // A token starting with '-' is an alt-name suffix ("Nine -L", "Pilot -3"), not a system
    // code — real codes always have content before the first hyphen ("C-J", "78-").
    if t.starts_with('-') {
        return false;
    }
    // A null-sec system name is 5 alphanumerics + one hyphen = 6 chars. Anything longer with a
    // hyphen ("skt-10001", "Jean-Luc") is a name, not a system (abbreviations are shorter).
    if t.len() > 6 {
        return false;
    }
    if !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || !t.chars().any(|c| c.is_ascii_alphanumeric())
    {
        return false;
    }
    // Code shape (case-insensitive): a digit ("C-J6MT", "Htg-0", "1dq1-a") or only short
    // (<=3 char) segments ("C-J"). A hyphenated all-letter token with a longer segment
    // ("Jean-Luc", "Mary-Jo") is a name, never a code.
    let has_digit = t.chars().any(|c| c.is_ascii_digit());
    let longest_segment = t.split('-').map(|s| s.len()).max().unwrap_or(0);
    has_digit || longest_segment <= 3
}

/// A short code-shaped token ("F2A", "f2a", "5E") — 2–5 alphanumerics carrying both a digit and a
/// letter (so plain words and bare counts are excluded). Used to resolve a bare, un-hyphenated
/// null-sec abbreviation against a neighbouring system (a "gate" callout to an adjacent system).
fn is_short_code_token(t: &str) -> bool {
    let n = t.chars().count();
    (2..=5).contains(&n)
        && t.chars().all(|c| c.is_ascii_alphanumeric())
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

/// A token shaped like a null-sec code but mixed-case and resolving to no real system (exact
/// or prefix) — far likelier a hyphenated pilot name ("Luo-xi") than a system we simply lack.
/// All-caps codes ("C-J") and any real/prefix-matched system are excluded, so a genuine
/// abbreviation isn't mistaken for a player.
fn is_code_lookalike_name(t: &str, systems: &Systems) -> bool {
    looks_like_system_code(t)
        && t.chars().any(|c| c.is_ascii_lowercase())
        && resolve(systems, t).is_none()
        && systems.lookup_prefix(t).is_none()
}

/// An anomaly / cosmic-signature id shape ("ABC-123", "ABCD-12", "ABC123"): at LEAST three
/// leading letters, then only a hyphen and/or digits, and at least one digit (so a plain word
/// like "cleared" is rejected). Deliberately stricter than [`looks_like_system_code`] — the
/// >=3-letter minimum keeps short null-sec codes ("C-J", "5E") out — and it's only ever consulted
/// next to an "anom"/"sig" keyword. A code that resolves to a real system is still a system: the
/// callers gate this against the real-systems lookup.
fn looks_like_anom_code(t: &str) -> bool {
    let n = t.chars().count();
    if !(3..=8).contains(&n) || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }
    let leading_letters = t.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    leading_letters >= 3
        && t.chars().skip(leading_letters).all(|c| c == '-' || c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_digit())
}

/// "Diamond rats" / "dia rat" — the dangerous NPC pirate variant, NOT a pilot. True when
/// "diamond"/"dia" is immediately followed by "rat"/"rats"; returns the words to consume so
/// none become a pilot name.
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

/// Anomaly / signature callouts: an "anom"/"sig" keyword ADJACENT (either order) to an anomaly-id
/// code ([`looks_like_anom_code`]) that does NOT resolve to a real (or neighbouring) system. Returns
/// the (kind, CODE) pairs and the keyword+code words to consume (so the code is never a pilot or a
/// standalone system, and the bare keyword is dropped).
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
        for j in [i.checked_sub(1), Some(i + 1)].into_iter().flatten() {
            let Some(code) = tokens.get(j) else { continue };
            if !looks_like_anom_code(code) {
                continue;
            }
            // A code that resolves to a real (or prefix/neighbour) system is a location, not an
            // anom/sig — reuse the authoritative real-systems lookup, never the shape test alone.
            if is_system_token(code, systems)
                || resolve(systems, code).is_some()
                || systems.lookup_prefix(code).is_some()
            {
                continue;
            }
            let upper = code.to_uppercase();
            if !out.iter().any(|(_, c)| c.eq_ignore_ascii_case(&upper)) {
                out.push((kind, upper));
                consumed.push(tokens[i].to_lowercase());
                consumed.push(tokens[j].to_lowercase());
            }
        }
    }
    (out, consumed)
}

/// A number glued to a time unit ("4min", "30s", "2h", "5m") — an ESS/timer duration,
/// not a name. Only leading digits count, so a handle like "0xtomorrow" is unaffected.
fn is_time_token(t: &str) -> bool {
    let lower = t.to_lowercase();
    let Some(de) = lower.find(|c: char| !c.is_ascii_digit()) else {
        return false; // all digits, no unit
    };
    if de == 0 {
        return false; // no leading digits
    }
    matches!(
        &lower[de..],
        "min" | "mins" | "minute" | "minutes" | "m" | "s" | "sec" | "secs"
            | "second" | "seconds" | "h" | "hr" | "hrs" | "hour" | "hours" | "d"
    )
}

/// A number glued to an ISK magnitude suffix ("346mio", "300kk", "1.5b", "750m") — an amount,
/// never a name. Only leading digits (with an optional decimal) count, so a real digit-bearing
/// handle like "01XcerberusX01" or "PORTOS11" is unaffected.
fn is_amount_token(t: &str) -> bool {
    let lower = t.to_lowercase();
    let de = match lower.find(|c: char| !c.is_ascii_digit() && c != '.') {
        Some(0) | None => return false, // no leading digits, or all digits (a bare count)
        Some(de) => de,
    };
    matches!(
        &lower[de..],
        "k" | "kk" | "m" | "mil" | "mill" | "million" | "millions" | "mio" | "mio."
            | "b" | "bil" | "bill" | "billion" | "billions" | "isk"
    )
}

/// A lower/digit-leading handle ("0xtomorrow", "xX1Mortis"): contains a digit and a
/// run of at least three letters, so it is name-shaped even without a Title-case first
/// letter. Excludes system codes (hyphen, no letters) and ISK/count tokens like "334m".

/// A single token distinctive enough to be a name candidate on its own (worth an
/// ESI lookup): a hyphen/apostrophe, internal capital ("SokoleOko"), or a digit —
/// patterns that plain words/ship names don't have.
fn is_distinctive_name(t: &str) -> bool {
    name_part(t)
        && !looks_like_system_code(t)
        && (t.contains('-')
            || t.contains('\'')
            || t.chars().skip(1).any(|c| c.is_ascii_uppercase())
            || t.chars().any(|c| c.is_ascii_digit()))
}

/// Replace each parenthesised span's contents with spaces (so a drag-drop ship name
/// inside "(…)" isn't mistaken for a pilot by the general heuristic).
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

/// Drag-and-drop dscan reports are always "<pilot> (<ship>)". Return each
/// (pilot, ship-text) pair — catches single-word/lower-case names like "SokoleOko"
/// and the (possibly non-English) ship inside the parentheses.
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
        // The "(ship)" context proves the preceding run is a pilot name, so accept
        // any case (catches "bigfoott"); skip all-caps system codes (e.g. "0UBC-R")
        // and status keywords.
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

/// Victim pilot + ship from a pasted killmail-link display string: `<killword><colon> <Victim>
/// (<Ship>)` - "Kill: Lord Road (Loki)", Chinese "击杀：Lord Road (洛基级)". The chat log strips the
/// `<url=killReport...>` tag, leaving only this text, and in some locales the killword+colon glues to
/// the first name word with no space ("击杀：Lord Road" is one whitespace token), so the victim never
/// forms via the normal paths. Read it directly: after the first `KILL_WORDS` match skip a colon
/// (ASCII `:` or fullwidth `：`) + spaces, take the name up to the next `(` (capped to 3 words), then
/// the parenthesised hull as the ship. Case is not consulted.
fn extract_kill_drops(text: &str) -> Option<(String, Option<String>)> {
    let lower = text.to_lowercase();
    let (kw_start, kw) = KILL_WORDS
        .iter()
        .filter_map(|kw| lower.find(kw).map(|i| (i, *kw)))
        .min_by_key(|&(i, _)| i)?;
    // `.get` is a no-op if the lowercased byte offset isn't a char boundary in the original (rare
    // length-changing lower-case before the killword) - then we simply bail.
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

/// Whether a double-space-delimited paste segment looks like a (single) pilot name. Names may be
/// lowercase, so the disqualifiers are: nothing alphabetic, all words being stop words, an
/// embedded solar system (a real name wouldn't carry one — that's prose), or a lowercase
/// intel-descriptor stop word ("in", "jumped"). Title-case stop words allowed inside names
/// ("The", "Blue") are fine. Used only to decide whether a double-space block is a clean paste.
fn segment_is_name(seg: &str, systems: &Systems) -> bool {
    let words: Vec<&str> = seg.split_whitespace().collect();
    // EVE names are >= 3 characters; the length floor also drops casual double-spaced chat
    // ("he  hi", "u  gg") from being mistaken for a paste.
    if words.is_empty()
        || seg.chars().filter(|c| !c.is_whitespace()).count() < 3
        || !seg.chars().any(|c| c.is_alphabetic())
    {
        return false;
    }
    // A segment is a name if it carries a real name part, embeds no solar system, and holds at most
    // ONE bare intel keyword. A single keyword inside a linked name ("fliet98 cyno") is kept whole
    // for ESI to confirm; MULTIPLE keyword/descriptor words are prose ("is clear now lads") and bail
    // the paste. Case is not consulted (per the parser rule); the keyword COUNT is.
    let bad_keyword =
        |w: &str| is_pilot_stopword(w) && !is_name_connector(w) && !is_name_capable_stopword(w);
    words.iter().any(|w| !is_pilot_stopword(w))
        && !words.iter().any(|w| resolve(systems, w).is_some())
        && words.iter().filter(|w| bad_keyword(w)).count() <= 1
}

/// Trim a trailing location phrase typed after a pasted name in one double-space segment
/// ("Garen Willow at taj" → "Garen Willow"). Only a locational preposition that FOLLOWS a >= 2-word
/// name starts the prose, so a 1-word-prefix name that legitimately contains one ("Man in Black")
/// is left intact.
fn trim_paste_location_tail(seg: &str, ship_index: &HashMap<String, (i64, String)>) -> String {
    const LOC_PREP: &[&str] = &["at", "in", "on", "near"];
    let mut words: Vec<&str> = seg.split_whitespace().collect();
    // Drop a trailing decorated count ("01XcerberusX01 +3" → "01XcerberusX01"): a "+N"/"xN"/"N+"
    // is the hostile count, parsed separately, and is never part of the pasted name.
    while words.len() >= 2 && is_decorated_count(words[words.len() - 1]) {
        words.pop();
    }
    // Drop trailing intel keyword/status tags AND a trailing hull note ("Ben Walker NV" → "Ben
    // Walker", "Roadman HighSec CynoLighter likely prospect" → "Roadman HighSec CynoLighter"): a
    // paste segment can carry the reporter's typed note (a status word or the pilot's ship) glued to
    // the linked name. A bare stop word, or a hull, on its own is that note, not part of the name —
    // strip it so the name resolves (and the freed word is read as its own flag/ship). Name-capable
    // words ("Clear", "Blue"), connectors ("of"), and initial/number suffixes are kept so a real
    // name ending in one survives; the single-space form already parses via the same trimming +
    // ship masking.
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

/// A decorated approximate-count token: `+3`, `x4`, `4x`, `3+` (digits plus a leading/trailing
/// `+`/`x`). Unambiguous — unlike a bare trailing number, which can be part of a name ("Malcolm 41").
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
            // Refuse only a run that is entirely stop words: a single lower-cased stop word, or
            // a group made up of nothing but single-word stop words. A real (sub-)name is kept
            // even when it's lower-cased ("wuming"), so it can anchor the full name.
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

/// A deterministic mirror of [`crate::pilot::PilotCache::cover`] for the KNOWN (already
/// ESI-confirmed) cache: if an over-glued loose run is just a confirmed pilot name extended by
/// system/code tokens — a held "name + location" blob like "Ruston Shackleford B-3QPD" — return
/// that known sub-name. The blob must NOT survive (it would hold the location inside the name),
/// so the caller surfaces the known name and frees the system token for the location pass. Only a
/// null-sec code / system can be the leftover (those never belong inside a character name); a
/// non-system extra word means we can't be sure, so the held model still waits on ESI.
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
    // Longest confirmed sub-span first, so the fullest known name wins ("Ruston Shackleford"
    // over a coincidental single "Ruston").
    for len in (1..=n).rev() {
        for start in 0..=n - len {
            // There must be a leftover token to free — otherwise the run IS the name, not a
            // held "name + location" blob.
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

/// Whether an over-glued loose run decomposes ENTIRELY into pilot candidates already
/// surfaced (each a contiguous run of the blob) plus bare system tokens — a single-space
/// kill-list ("A B C D System") whose individual names the known-cache / strict passes have
/// already found. Such a blob adds nothing but a mega-candidate that the final sub-phrase
/// drop would let EVICT those clean names (they're all its sub-phrases), so the caller skips
/// it. Greedy longest-match, so a two-word name is preferred over its first word.
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

/// Stop words that still appear inside real multi-word names ("The Meek", "Lord of War")
/// and so are allowed mid-name — unlike intel descriptors ("cloaked", "jumped", "camped"),
/// which are stop words that never belong in a name.
fn is_name_connector(w: &str) -> bool {
    matches!(
        w.to_lowercase().as_str(),
        "the" | "of" | "and" | "for" | "von" | "van" | "de" | "del" | "di" | "da"
            | "la" | "le" | "el" | "der" | "den" | "du" | "lord"
    )
}

/// Intel keywords that are stop words on their own ("3 reds in local", "bubble up") but also
/// appear inside real character names ("Blue RandomAttac", "The Bubble Boy"). They are allowed
/// inside a loose run so the full span reaches the ESI cover, which confirms or splits it —
/// instead of breaking the run and gluing the rest into an unsplittable blob. When the span
/// becomes a pilot candidate, its tokens also suppress the matching status flag (so "The
/// Bubble Boy" no longer reads as a warp-bubble sighting).
fn is_name_capable_stopword(w: &str) -> bool {
    matches!(
        w.to_lowercase().as_str(),
        "blue" | "blues" | "red" | "reds" | "bubble" | "bubbles" | "clear" | "autopilot"
    )
}

/// A short name component that can't stand alone but is valid inside a name: a single
/// capital initial ("Lopatich R") or a short number ("Adama 80", "Malcolm 41"). Only
/// ever extends a run that already has a real name word; never starts one.
fn is_name_suffix(t: &str) -> bool {
    // A single capital letter is an initial — except "I", which is the English pronoun and
    // would otherwise glue "I think", "I guess" into phantom names. Names that genuinely
    // contain "I" do so as a letter inside a word ("Iris", "I-Pustelga") or via an
    // authoritative showinfo link, neither of which relies on this rule.
    (t.len() == 1 && t.starts_with(|c: char| c.is_ascii_uppercase()) && t != "I")
        || (matches!(t.len(), 1..=4) && t.chars().all(|c| c.is_ascii_digit()))
        // Alt suffix attached to a name: "-3", "-L", "-42".
        || (t.starts_with('-')
            && matches!(t.len(), 2..=4)
            && t[1..].chars().all(|c| c.is_ascii_alphanumeric()))
}

fn extract_pilots(text: &str) -> Vec<String> {
    let is_namepart = name_part;
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, out: &mut Vec<String>| {
        // Connector stop words are fair game inside a multi-word name ("The Meek"), as are
        // name-capable keywords when Title-cased ("Clear Rain", "Blue Skies"). A run is rejected
        // if it's all stop words OR contains a genuine intel descriptor ("cloaked").
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

/// A ship hull from a lower-cased token, also accepting a simple plural so intel like
/// "tengus" / "lokis" / "drakes", the "-ies" form "harpies" -> "harpy", or the "-es" form on a
/// sibilant stem "areses" -> "ares", resolves to the hull (and isn't read as a pilot name).
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

/// A recognised non-name entity that ALWAYS ends a name run — a ship, time, structure,
/// wormhole code, cap/tackle keyword, or a token with characters a name can't have. Stop
/// words are NOT breakers (a name may embed them, "Cult is Dead"); systems/codes are handled
/// separately — they break only when not flanked by a name word (see `loose_pilot_runs`).
fn hard_name_breaker(core: &str, ship_index: &HashMap<String, (i64, String)>) -> bool {
    let lc = core.to_lowercase();
    core.is_empty()
        || !core.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
        || is_cap_word(&lc)
        || is_tackle_word(&lc)
        || is_time_token(core)
        || is_structure_word(core)
        || crate::wormholes::is_wh_code(core)
        || ship_of(&lc, ship_index).is_some()
}

/// True if a candidate name blob still contains a system/code token — a location *held* inside a
/// name, pending ESI. Such a report is parked (kept) rather than dropped for lacking a location,
/// so the reconcile can re-derive the location once the name resolves and frees the token.
pub(crate) fn has_held_system(report: &IntelReport, systems: &Systems) -> bool {
    report
        .pilots
        .iter()
        .flat_map(|p| p.split_whitespace())
        .any(|w| is_system_token(w, systems))
}

/// A system name or null-sec code (but not a code-shaped *name* like "Luo-xi").
fn is_system_token(core: &str, systems: &Systems) -> bool {
    (looks_like_system_code(core) && !is_code_lookalike_name(core, systems))
        || systems.lookup(core).is_some()
}

/// A plausible real name word: not a hard entity, not a system, not a stop word, >=3 letters.
/// Used to decide whether an adjacent system token is part of a name ("Bob Uitra") or stands
/// alone as a location/gate ("N3-JBX Uitra", "hostiles in Jita").
fn is_name_anchor(core: &str, ship_index: &HashMap<String, (i64, String)>, systems: &Systems) -> bool {
    !hard_name_breaker(core, ship_index)
        && !is_system_token(core, systems)
        && !is_pilot_stopword(core)
        && core.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
}

/// Maximal runs of name-material tokens (broken only by recognised non-name entities — see
/// [`breaks_name_run`]), kept whole so the full multi-word name reaches ESI. Stop words are
/// kept inside a run ("Cult is Dead"); a run is dropped only when it is ENTIRELY lower-case
/// stop words (prose like "gate is camped"). The ESI permutation resolver ([`PilotCache::cover`])
/// claims the real characters inside each blob, longest match first.
fn loose_pilot_runs(
    text: &str,
    ship_index: &HashMap<String, (i64, String)>,
    systems: &Systems,
) -> Vec<String> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let flush = |run: &mut Vec<String>, out: &mut Vec<String>| {
        // Trim only short connective/filler stop words ("in", "the", "nv") and lone letters off
        // the BOUNDARIES — never a longer content word. A real name can end in a content word
        // that is also a keyword ("High Plains Drifter"), so those stay and the ESI cover decides;
        // stop words otherwise only discard a candidate that is ENTIRELY prose (all-stop below).
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
        // An OVER-LENGTH run (>3 words) can never be a name, so a boundary token that is not name
        // material is prose/anchor that pushed a valid <=3-word name over the limit — trim it until
        // the run is a plausible name length. This catches a non-name-capable stop word ("Roadman
        // HighSec CynoLighter likely") and a leading/trailing system or wormhole code that glued in
        // the single-space form ("DUO-51 Roadman HighSec CynoLighter"). A content keyword that
        // legitimately ends a <=3-word name ("High Plains Drifter") is untouched — this only fires
        // while the run is still longer than 3 words.
        let over_trim = |w: &String| {
            (is_pilot_stopword(w) && !is_name_connector(w) && !is_name_capable_stopword(w))
                || crate::wormholes::is_wh_code(w)
                || (looks_like_system_code(w) && !is_code_lookalike_name(w, systems))
        };
        while run.len() > 3 && run.last().is_some_and(&over_trim) {
            run.pop();
        }
        while run.len() > 3 && run.first().is_some_and(&over_trim) {
            run.remove(0);
        }
        // An over-length run may still carry a typed location tail after the name ("Garen Willow at
        // taj"); cut it at a locational preposition that FOLLOWS a >=2-word name, mirroring
        // `trim_paste_location_tail` so the single-space form matches the pasted form.
        if run.len() > 3 {
            const LOC_PREP: &[&str] = &["at", "in", "on", "near"];
            if let Some(cut) = run
                .iter()
                .enumerate()
                .find(|(i, w)| *i >= 2 && LOC_PREP.contains(&w.to_lowercase().as_str()))
                .map(|(i, _)| i)
            {
                run.truncate(cut);
            }
        }
        // A whole name is at least 3 letters (not each word — "Bo Li" is fine), and is not
        // ENTIRELY lower-case stop words — a capital makes even an all-stop-word run
        // ("Clear Rain") worth an ESI check.
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
    // A token strong enough to be a real name on its own: a mixed-case Title-Case word that
    // isn't a stop word ("Micahel", "Htguuu" — but not "Dead" or a system code). Used to bound
    // the conservative stray-WORD breaker below.
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
            // A lone single letter is never a name component on its own — a mistyped "v" the
            // reporter dropped mid-list, or the pronoun "I". (A real one-letter initial like
            // "Lopatich R" is a capital `is_name_suffix` and is kept.) Treat it as an internal
            // boundary: split the run and drop the letter, so the names on EITHER side parse
            // independently instead of gluing into one over-long blob.
            true
        } else if is_pilot_stopword(core)
            && !is_name_connector(core)
            && !is_name_capable_stopword(core)
            && !name_part(core)
            && prev.is_some_and(|w| is_strong_name(w))
            && next.is_some_and(|w| is_strong_name(w))
        {
            // A stray chatter word ("lol", "pls") wedged BETWEEN two real names is junk the
            // reporter typed mid-list, not part of either name — split here so both names parse.
            // Conservative on purpose: only a stop word that can't anchor a name AND is flanked
            // on BOTH sides by strong Title-Case names breaks, so an embedded connector ("Cult
            // is Dead", "Lord of War") is left whole for the ESI cover to judge.
            true
        } else if is_system_token(core, systems) {
            // A system/code breaks UNLESS flanked by a real name word — then it may be part of a
            // name ("Bob Uitra", "G-EURJ Keeves") and is kept for ESI to confirm; "hostiles in
            // Jita" / "N3-JBX Uitra" have no adjacent name word, so the system stands alone.
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

/// Multi-word hull names ("Exequror Navy Issue", "Stabber Fleet Issue") matched
/// against the full ship name, longest run first. Returns (start_word, len, id,
/// name). Checked before pilot detection so they aren't read as 3-word names.
fn multiword_ships(
    text: &str,
    ship_index: &HashMap<String, (i64, String)>,
    known_pilots: &HashMap<String, i64>,
) -> Vec<(usize, usize, i64, String)> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
    // Multi-word hull names (2..=4 words), pre-split lower-case, for the conservative typo
    // fallback below.
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
            // Exact hull, or one with the trailing "Issue" dropped ("Brutix Navy" -> Brutix Navy
            // Issue, "Stabber Fleet" -> Stabber Fleet Issue), or the whole faction suffix
            // abbreviated ("Vexor NI" -> Navy Issue, "Stabber FI" -> Fleet Issue).
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
            // The phrase, or a simple plural de-pluralised: "-ies" -> "-y" ("osprey navies" ->
            // "osprey navy"), "-es" for a sibilant stem ("areses" -> "ares"), or plain "-s"
            // ("osprey navys"). Try each in that order and keep the first that actually resolves to
            // a hull (so "bellicoses" -> "bellicos" fails the "-es" try and falls through to "-s"
            // -> "bellicose"). Case is not consulted.
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
        // Typo tolerance (LAST RESORT — only when no exact hull matched at this position): a
        // window matches a multi-word hull when every word but one is EXACT and the single odd
        // word is edit-distance <= 1 from the hull's word (both reasonably long), the hull is
        // unambiguous, and the window isn't itself a confirmed pilot. "cythe fleet issue" ->
        // Scythe Fleet Issue (only "cythe"->"scythe" differs; "fleet"+"issue" match exactly).
        if !matched {
            for len in (2..=max).rev() {
                let win: Vec<String> =
                    words[i..i + len].iter().map(|w| w.to_lowercase()).collect();
                // A confirmed real pilot whose name is one edit from a hull stays a pilot.
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
                        // Exactly one typo'd word; it must be long enough that a single edit is
                        // distinctive (>= 5 chars, never shrinking below 4) — short common words
                        // ("navy", "the") are NEVER fuzzed into a hull word.
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

/// Known MULTI-WORD solar-system names ("Sanctified Vidette" and the other Drifter
/// wormhole systems built on Barbican/Conflux/Redoubt/Sentinel/Vidette). Almost every EVE
/// system is a single token, so the parser treats systems as single tokens — but the few
/// multi-word names would otherwise be read as a 2-word pilot. This scans adjacent word
/// windows (longest first, non-overlapping) and keeps a window that EXACTLY matches a known
/// system name (case-insensitive — `Systems::lookup` lower-cases). The full system set the
/// app already loaded is the source of truth (no hard-coded list); a window only matches when
/// it is a real multi-word system, so an unrelated 2-word pilot ("John Smith") is untouched.
/// Returns (start_word, len, id, name), mirroring [`multiword_ships`].
fn multiword_systems(text: &str, systems: &Systems) -> Vec<(usize, usize, i64, String)> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
    let mut out: Vec<(usize, usize, i64, String)> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut adv = 1;
        // Longest window first (up to 4 words) so an "adjective complex" name wins over any
        // shorter accidental prefix. A single-word system name can never match a >=2-word
        // window (it has no space), so only a genuine multi-word system is picked up here.
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

/// Drop a detected name that only ever appears as the leading words of a longer detected
/// name in the same text ("Gallente Citizen" inside "Gallente Citizen 17120704"). Both can
/// be real characters, but only the longer one was actually mentioned here. A name that
/// also appears on its own (count exceeds the longer names that contain it) is kept.
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

/// Candidate "Title-Case + one lower-case word" names ("Psychopathic beemaster") —
/// EVE family names can be lower-case. Only when the first word isn't a system and
/// the second isn't a ship/keyword; ESI confirmation filters false positives.
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

/// A known single-word character preceded by a plain lower-cased word is the full name relayed
/// as plain text ("ji wuming", where only "wuming" is in the cache). Grab the leading word so
/// the real name isn't truncated to its surname — as long as that word is an ordinary lower-case
/// word (not a stop word, system, ship, or a known name in its own right).
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
        // b is a known single-word character; a is a plain lower-cased leading word.
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

/// A plain lower-cased first name followed by a Title-case surname that is itself a system name
/// ("alexpanda Uitra") — the surname would otherwise be mis-detected as a second system and
/// demoted to a bogus gate. Only fires when the message already names another system (so the
/// report's location is established and the system-named word is clearly a surname).
fn lowercase_lead_system_names(
    text: &str,
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
) -> Vec<String> {
    let punct = |c: char| ",.;:!?\"()".contains(c);
    let words: Vec<&str> =
        text.split_whitespace().map(|w| w.trim_matches(punct)).filter(|w| !w.is_empty()).collect();
    // Corroboration: at least two system-looking tokens, so one besides the surname is the
    // report's actual location.
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
        // b: a Title-case word that resolves to a system (the surname being mis-detected).
        let b_ok = name_part(b) && !is_pilot_stopword(b) && resolve(systems, b).is_some();
        if a_ok && b_ok {
            out.push(format!("{a} {b}"));
        }
    }
    out
}

/// Drop a pilot that is a contiguous sub-phrase of a longer one (e.g. "Nine" when "Nine -3"
/// is also present) — used after a merge, since each message is filtered individually.
/// A `protect`ed name (one with an authoritative showinfo char-id) is never dropped: a
/// glued mis-parse from a plain-text relay must not evict the real, char-linked name.
fn drop_subphrase_pilots(
    pilots: &mut Vec<String>,
    protect: &std::collections::HashSet<String>,
    source: &str,
) {
    let lc: Vec<String> = pilots.iter().map(|p| p.to_lowercase()).collect();
    // Each pilot as its lower-cased token sequence (so occurrence counting matches the way the
    // source text is tokenised: hyphen/apostrophe kept, punctuation split).
    let toks: Vec<Vec<String>> =
        pilots.iter().map(|p| tokenize(p).iter().map(|t| t.to_lowercase()).collect()).collect();
    let src: Vec<String> = tokenize(source).iter().map(|t| t.to_lowercase()).collect();
    // Non-overlapping count of `needle` as a contiguous run inside `hay`.
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
            // Longer candidates that contain this one as a contiguous sub-phrase.
            let longer: Vec<usize> = (0..pilots.len())
                .filter(|&j| {
                    j != i
                        && lc[j].len() > lc[i].len()
                        && format!(" {} ", lc[j]).contains(&format!(" {} ", lc[i]))
                })
                .collect();
            if longer.is_empty() {
                return true; // not a sub-phrase of anything → always kept
            }
            // Occurrence-aware: keep the shorter name only when the source text has at least one
            // separate token occurrence of it BEYOND the ones already consumed by the longer names
            // that contain it. "Tiffanbrill Tiffanbrill Dragon" has two "Tiffanbrill" tokens and one
            // "Tiffanbrill Dragon", so the standalone pilot and the two-word pilot both stand. A
            // genuine over-glue — one "Ruston Shackleford" inside a single "Ruston Shackleford
            // B-3QPD" — has no spare occurrence, so it still collapses. With no source occurrences
            // (e.g. a cross-report merge whose texts don't share the name) total is 0, matching the
            // old unconditional drop.
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

/// Pre-clean intel text before parsing: drop EVE's "*" route-waypoint marker (so a marked
/// system like "NB-ALM*" still resolves), and strip a re-pasted chat line's
/// "[ time ] Sender > " prefix when the body is an in-game-link paste (the inner sender is
/// not a hostile).
fn preprocess_intel(text: &str) -> String {
    let mut t = text.trim();
    if t.starts_with('[') {
        if let Some(i) = t.find(']') {
            t = t[i + 1..].trim_start();
        }
    }
    t.replace('*', "")
}

/// Pilot → recent (system, time) sightings index (Phase 1 data layer).
///
/// Records where a named pilot was seen and when, so Phase 2 can tell whether a pilot
/// that some heuristic would demote has actually been "revived" — i.e. is roaming
/// across several systems right now. Keyed by lower-cased pilot name; entries older
/// than the 4h window are pruned.
#[derive(Default)]
pub struct Sightings {
    map: HashMap<String, Vec<(i64, i64)>>,
}

/// Retention window for sightings (4h), in seconds.
const SIGHTINGS_WINDOW: i64 = 14400;

pub type SharedSightings = std::sync::Arc<std::sync::Mutex<Sightings>>;

impl Sightings {
    /// Record that `name` was sighted in `system_id` at `ts` (unix seconds).
    pub fn record(&mut self, name: &str, system_id: i64, ts: i64) {
        if system_id <= 0 {
            return;
        }
        self.map.entry(name.to_lowercase()).or_default().push((system_id, ts));
    }

    /// Drop sightings older than the 4h window; drop names left empty.
    pub fn prune(&mut self, now: i64) {
        let cutoff = now - SIGHTINGS_WINDOW;
        self.map.retain(|_, v| {
            v.retain(|&(_, ts)| ts >= cutoff);
            !v.is_empty()
        });
    }

    /// Distinct systems a pilot was sighted in with `ts >= now - window`.
    /// (Used in Phase 2.)
    #[allow(dead_code)] // used in Phase 2
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

    /// Whether a pilot is roaming widely enough to be considered "revived":
    /// 3+ distinct systems in the last hour, or 5+ in the last 4h. (Used in Phase 2.)
    #[allow(dead_code)] // used in Phase 2
    pub fn revived(&self, name: &str, now: i64) -> bool {
        self.distinct_systems_since(name, 3600, now) >= 3
            || self.distinct_systems_since(name, SIGHTINGS_WINDOW, now) >= 5
    }
}

/// Analyse one message into a structured report (movement is added later).
#[allow(dead_code)] // thin no-context wrapper, kept for the public API + tests
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
    "kill:",       // English
    "击杀", // Chinese - kill
    "损失", // Chinese - loss
    "キル", // Japanese - kill
    "킬",       // Korean - kill
    "abschuss",     // German
    "убийство", // Russian - kill
];

/// Extract the EVE region names a channel covers from its MOTD
/// ("Channel MOTD: TENERIFIS // IMMENSEA // …"). Only names matching a known region are
/// kept, so the trailing prose is ignored. The result is a hint for abbreviation
/// disambiguation, not an absolute filter.
pub fn parse_motd_regions(motd: &str, known: &std::collections::HashSet<String>) -> Vec<String> {
    // Scan from the MOTD marker onward (skip the log header) for *any* known region
    // name, regardless of separators or line breaks. It's only a disambiguation hint, so
    // being inclusive is fine. A boundary check (not preceded by a letter, not followed
    // by a lower-case letter) keeps "Catch" out of the middle of words while still
    // catching "CATCHPlease" where regions are glued to the trailing prose.
    let body = match motd.rfind("Channel MOTD:") {
        Some(i) => &motd[i + "Channel MOTD:".len()..],
        None => motd,
    };
    // Scan the original bytes with a case-insensitive compare so the boundary checks
    // read original case at correct offsets (lowercasing isn't byte-length-preserving for
    // non-ASCII MOTD prose, which would desync a separate lowercased copy). Region names
    // are ASCII, so byte indexing here never splits or panics on multi-byte chars.
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
                // A lower-case letter right after means we're mid-word ("Catchy"); an
                // upper-case letter or non-letter is a new token ("CATCHPlease").
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
/// Detect the report's location — solar systems, the gate destination, and the tokens consumed
/// doing so. `reserved` holds tokens already claimed by a name (skipped). Extracted from
/// `analyze_ctx` so the post-ESI reconcile can re-run it once confirmed names free their tokens
/// (the held model: a system inside a name blob is held, then re-derived if the name is rejected).
#[allow(clippy::too_many_arguments)]
pub(crate) fn detect_location(
    tokens: &[&str],
    lower_tokens: &[String],
    reserved: &std::collections::HashSet<String>,
    systems: &Systems,
    context_system: Option<i64>,
    channel_regions: &[String],
) -> (Vec<DetectedSystem>, Vec<String>, Vec<String>) {
    let pilot_tokens = reserved; // alias so the moved body reads unchanged
    let mut detected: Vec<DetectedSystem> = Vec::new();
    // Tokens consumed as systems/gates must not also be counted (e.g. "78" in
    // "on 78 gate" is a gate, not 78 hostiles).
    let mut consumed: Vec<String> = Vec::new();
    // A bare 1–2 digit number is ambiguous: it could be a system/gate code prefix
    // (e.g. "78" → 78-) or a hostile count (e.g. "10 neut"). Defer these and accept
    // them as a system only if they're a direct neighbour of a named system.
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
    // Neighbours of the confidently-named systems.
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

    // Standalone null-sec abbreviation ("C-J" when both C-J6MT and C-J7CR exist, with
    // no "gate" word): resolve by prefix against an already-named system's neighbours or
    // the channel context. Prefix-only and hyphenated-code-only, so plain words and
    // suffixes ("6MT") never match.
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
                // Hint: an Imperium channel's regions (from its MOTD) pick C-J6MT over the
                // Vale of the Silent C-J7CR even with no nearby named system.
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

    // Secondary-system gate callout: a short code-shaped token with a digit ("F2A", "f2a", "5E")
    // that prefix-matches a NEIGHBOUR of the primary system is a gate to that adjacent system —
    // a real neighbour is a strong enough signal to reclaim the token even if it was tentatively
    // read as a name (these intel callouts name the next system over). The neighbour set is small,
    // so even a 2-char prefix is unambiguous. Case-insensitive. Resolved as a system here; the
    // gate-demotion pass below turns an adjacent secondary system into a gate.
    {
        let primary = detected.first().map(|d| d.id).or(context_system);
        if let Some(p) = primary {
            for tok in tokens.iter() {
                let lc = tok.to_lowercase();
                if consumed.contains(&lc)
                    || looks_like_system_code(tok) // hyphenated codes handled above
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

    // Gate: "... <System> gate" — hostiles are on the gate *to* <System>. Record it
    // (resolved name, or the raw token if abbreviated/unknown) and don't also list
    // it as a plain system.
    let mut gate: Option<String> = None;
    // Prefer a system named in this message; otherwise fall back to the channel's
    // last-known system so a bare "C-J gate" still resolves against its neighbours.
    let primary = detected.first().map(|d| d.id).or(context_system);
    for (i, tok) in tokens.iter().enumerate() {
        if !tok.eq_ignore_ascii_case("gate") || i == 0 {
            continue;
        }
        let cand = tokens[i - 1];
        if cand.eq_ignore_ascii_case("on") || cand.eq_ignore_ascii_case("the") {
            continue;
        }
        // A system-named word that belongs to a pilot NAME is that pilot's surname, not a gate —
        // "alexpanda Uitra gate" / "Bob Uitra gate" is the pilot "… Uitra", never a Uitra gate.
        // Recognised by a genuine name word immediately before it that is also reserved by the
        // name (raw parse: still inside the candidate; reconcile: reserved once ESI confirms the
        // name). A code-shaped gate ("78-", "C-J") or an abbreviation preceded by a system/stop
        // word ("on 78- gate", "C-J6MT YPW gate", "N3-JBX Uitra gate") has no such name
        // predecessor, so it still resolves as the gate.
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
        // An explicit "<x> gate" keyword is authoritative for a resolvable code, but
        // a bare number that doesn't resolve is never a gate name (a single digit
        // never is, and e.g. "5 gate" means five hostiles).
        let resolved = resolve(systems, cand)
            .or_else(|| {
                // The gate leads to a *neighbour* of the report's system, and there are
                // only a handful — so even a 1–2 char prefix is unambiguous: "C-J6MT >
                // 5e gate" → 5E-CFL. (A bare number is a hostile count, not a name.)
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
                // Still nothing: accept an unambiguous global abbreviation (e.g. "YPW").
                let abbrev = cand.len() >= 2
                    && cand.chars().all(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || c == '-');
                if abbrev { systems.lookup_prefix(cand) } else { None }
            });
        // "O3-4MN Gate camp" is the "gate camp" keyword, not "O3-4MN gate": don't demote
        // the report's own system to a gate when "gate" is immediately followed by "camp".
        if resolved.is_some()
            && resolved.map(|s| s.id) == primary
            && tokens.get(i + 1).is_some_and(|n| n.eq_ignore_ascii_case("camp"))
        {
            continue;
        }
        if resolved.is_none() && cand.chars().all(|c| c.is_ascii_digit()) {
            break;
        }
        gate = Some(resolved.map_or_else(|| cand.to_string(), |s| s.name.clone()));
        consumed.push(cand.to_lowercase());
        if let Some(info) = resolved {
            detected.retain(|d| d.id != info.id);
        }
        break;
    }

    // "Ansi"/"Ansiblex" = the Ansiblex jump bridge in the report's system; treat it as
    // the gate it leads to (the configured bridge's destination), so "camp on the Ansi"
    // points at the system the bridge reaches.
    if gate.is_none() && lower_tokens.iter().any(|t| t == "ansi" || t == "ansiblex") {
        if let Some(dest) = primary.and_then(|p| systems.jump_bridge_dest(p)) {
            detected.retain(|d| d.id != dest.id);
            gate = Some(dest.name.clone());
        }
    }

    // Wormhole-destination reference: a system named only as the far side of a wormhole
    // ("... Thera hole", "wh to Thera") is NOT where the activity is, so it must not become
    // the primary system or a gate. Drop it from `detected` when another system can be the
    // location. Thera / Turnur (the wandering-hole hubs) are wh references by name; any other
    // system counts when it is flanked by a wormhole connector word ("hole"/"wh"/"to <sys>").
    // The wh destination itself is still surfaced by `parse_wh_dest` on the message.
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
                    // "Thera hole" / "Thera wh" — the connector FOLLOWS the destination — or
                    // "wh to Thera" — the destination follows "to". A system that merely FOLLOWS
                    // a "hole" word ("Thera hole Rancer") is the location, not a wh reference.
                    && (tokens.get(i + 1).is_some_and(|n| wh_word(n))
                        || i.checked_sub(1)
                            .and_then(|j| tokens.get(j))
                            .is_some_and(|p| p.eq_ignore_ascii_case("to")))
            })
        };
        // Never empty the location: only demote wh references when a real location remains.
        if detected.iter().any(|d| !is_wh_ref(d)) {
            detected.retain(|d| !is_wh_ref(d));
        }
    }

    // One system per card: keep the first mentioned, and demote any further system
    // mentions to gates (a report can list several gates). Combined with any explicit
    // "<X> gate" already found above.
    let mut gates: Vec<String> = Vec::new();
    if let Some(g) = gate {
        gates.push(g);
    }
    if detected.len() > 1 {
        // A further system is only a gate if it's actually gate-adjacent to the primary.
        // A non-adjacent mention is a wormhole/route destination, not a gate ("R959-U WH to
        // Agaullores"), so don't demote it to one.
        let primary = detected[0].id;
        let adjacent: std::collections::HashSet<i64> =
            systems.neighbors(primary).iter().copied().collect();
        for d in detected.split_off(1) {
            // When we know the primary's gate neighbours and this system isn't among them,
            // it's a wormhole/route destination, not a gate. With no adjacency data we can't
            // know, so we fall back to demoting it.
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

/// As [`analyze`], but with the channel's last-known system as context so an
/// abbreviated gate ("C-J gate") can disambiguate against that system's neighbours
/// even when the message doesn't restate a system.
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
    // Capture pasted links (dscan / zKill / battle report) BEFORE stripping — `display_text` keeps
    // the original so the card can still show/click them.
    let links = extract_links(text);
    // A consumed URL must not be considered for any other parsing: blank every http(s) token out of
    // the text the parser sees, so its host/path/hash fragments aren't read as pilots/ships/systems.
    let stripped = strip_urls(text);
    let text = stripped.as_str();
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = tokenize(text);
    let lower_tokens: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();

    // Candidate pilot names first: their tokens must not be parsed as ships or
    // systems (player names often contain hull/system names, e.g. "Sabre Pilot" or
    // "Jita Trader"). Quoted spans are forced to be names.
    // Multi-word hull names are ships, not 3-word pilot names — find and mask them
    // before pilot detection.
    let mw_ships = multiword_ships(text, ship_index, known_pilots);
    // Known MULTI-WORD system names ("Sanctified Vidette") are masked before pilot detection —
    // like a single-token system, a multi-word system name must never surface as a pilot. They
    // are added to the detected-systems list after `detect_location` below.
    let mw_systems = multiword_systems(text, systems);
    // Structure spans ("Cyno Beacon", "Keepstar") are masked too, so a structure word
    // is never also read as a pilot ("Beacon"). Asteroid/ice belt spans are masked the same way
    // so "Belt"/"Ice Belt" is a location badge, never a pilot.
    let cel_words = structure_words(text);
    let struct_spans = structure_spans(&cel_words);
    let belt_spans = belt_locations(&cel_words);
    // Mask ship/structure/belt spans by blanking their characters in place (replacing with spaces)
    // rather than collapsing to single-spaced words — this PRESERVES the original whitespace, so a
    // paste's double-space entity delimiters survive into segmentation (a name/ship never contains
    // a double space).
    let masked_words: String = {
        // Byte span of each whitespace-delimited token, in split_whitespace order.
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
        // Structure and asteroid/ice-belt spans are masked the same way as ships.
        for (w, len, _) in struct_spans.iter().chain(belt_spans.iter()) {
            for k in *w..(*w + *len).min(spans.len()) {
                blank.push(spans[k]);
            }
        }
        text.char_indices()
            .map(|(i, c)| if blank.iter().any(|(s, e)| i >= *s && i < *e) { ' ' } else { c })
            .collect()
    };
    // Run the general heuristic with parenthesised spans (drag-drop ships) and the
    // multi-word ship spans masked, so neither is read as a pilot.
    let masked = mask_parens(&masked_words);
    let mut pilots = extract_pilots(&masked);
    for n in lowercase_tail_names(&masked, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    // "Word lowercase 1234" runs are pilot names even when the first word is a
    // system ("Amarr slave 3424") — the trailing number disambiguates a name.
    for n in numbered_names(&tokenize(&masked)) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    // Known (ESI-confirmed) names from the local cache — exact, case-insensitive.
    for k in match_known_pilots(&masked, known_pilots) {
        // A demoted-for-inactivity name (Phase 2) is treated exactly like a stop word: don't
        // anchor on it — leave its tokens free for keyword/ship/system detection.
        if denied.contains(&k.to_lowercase()) {
            continue;
        }
        // A standalone word that's a known ship is the ship ("Buzzard"); a null-sec
        // code is the system, not a player who happens to be named like it ("C-J").
        if (!k.contains(' ') && ship_index.contains_key(&k.to_lowercase()))
            // Block only a token that is REALLY a system (an all-caps code, or a code shape
            // confirmed by a systems lookup) — a code-shaped name with no real system
            // ("Zzz-9", "Htg-0") is a legitimate player, even from the known cache.
            || is_system_token(&k, systems)
            || is_time_token(&k)
            || is_structure_word(&k)
            // A code-shaped null-sec prefix ("88a" → 88A-RA) is the system, not a player
            // who happens to share that name.
            || (!k.contains(' ')
                && systems.lookup_prefix(&k).is_some_and(|s| looks_like_system_code(&s.name)))
        {
            continue;
        }
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&k)) {
            pilots.push(k);
        }
    }
    // A plain-text-relayed full name whose surname is the only cached part ("ji wuming") — grab
    // the leading word so the sub-name added above doesn't stand alone (the subphrase pass then
    // drops the bare surname in favour of the full name).
    for n in lowercase_known_compound(&masked, known_pilots, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    // "<lower-case firstname> <Title-case surname that is a system>" — keep the surname out of
    // system/gate detection ("alexpanda Uitra" must not list a Uitra gate).
    for n in lowercase_lead_system_names(&masked, systems, ship_index) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&n)) {
            pilots.push(n);
        }
    }
    // Drag-and-drop dscan "<pilot> (<ship>)": the name and the ship (by full name,
    // incl. translations) — catches single-word names and multi-word hull names.
    let mut drop_ships: Vec<(i64, String)> = Vec::new();
    for (pilot, ship_text) in extract_dscan_drops(text) {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&pilot)) {
            pilots.push(pilot);
        }
        if let Some((id, name)) = ship_index.get(&ship_text.to_lowercase()) {
            drop_ships.push((*id, name.clone()));
        }
    }
    // Quoted spans are forced to be names (so "'clear'" is a pilot, not a keyword).
    let quoted_raw = extract_quoted(text);
    let quoted: std::collections::HashSet<String> =
        quoted_raw.iter().map(|q| q.to_lowercase()).collect();
    for q in quoted_raw {
        if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&q)) {
            pilots.push(q);
        }
    }
    // Distinctive single tokens (hyphen/apostrophe/internal-caps/digit) that aren't
    // a system — candidates worth an ESI lookup (e.g. "I-Pustelga").
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
    // Drop a candidate that is a contiguous sub-phrase of a longer candidate (so a
    // short known name doesn't false-parse inside a longer reported name), and drop
    // a single-word candidate that is actually a ship (prefer the ship).
    let lc: Vec<String> = pilots.iter().map(|p| p.to_lowercase()).collect();
    pilots = pilots
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            let me = &lc[*i];
            let is_subphrase = lc.iter().enumerate().any(|(j, other)| {
                j != *i && other.len() > me.len() && format!(" {other} ").contains(&format!(" {me} "))
            });
            // A candidate that is exactly a hull name is the ship, never a pilot —
            // single- or multi-word (e.g. "Harbinger Navy Issue").
            let is_ship_name = ship_index.contains_key(me);
            // A status shorthand ("clr", "nv", …) is never a pilot, even if some
            // character happens to share that name.
            let single_stop = !p.contains(' ')
                && !quoted.contains(me)
                && (PILOT_STOP.contains(&me.as_str()) || CLEAR_WORDS.contains(&me.as_str()));
            !is_subphrase && !is_ship_name && !single_stop
        })
        .map(|(_, p)| p.clone())
        .collect();
    // A wormhole signature code (e.g. "K162") is never part of a pilot name.
    pilots.retain(|p| !p.split_whitespace().any(crate::wormholes::is_wh_code));
    // Loose runs (any-case, anchored by a Title-Case word) — added AFTER the sub-phrase
    // filter so they don't swallow the strict shorter names; the ESI cover confirms or
    // splits each later (non-names are dropped).
    for r in loose_pilot_runs(&masked, ship_index, systems) {
        if pilots.iter().any(|p| p.eq_ignore_ascii_case(&r)) {
            continue;
        }
        // An all-stop-word run ("they are", "back to", "still here") is sentence-capitalised
        // prose, never a pilot — drop it before it reaches ESI (loose_pilot_runs keeps a
        // capitalised all-stop run for the cover to judge; here we know it's not a name).
        if is_pilot_stopword(&r) {
            continue;
        }
        // Demoted-for-inactivity (Phase 2): skip like a stop word so the run's tokens stay free.
        if denied.contains(&r.to_lowercase()) {
            continue;
        }
        // An over-glued blob whose every word already belongs to a shorter candidate we've
        // surfaced (a single-space kill-list "A B C D System", each name found by the known
        // cache) plus system tokens adds nothing — and the final sub-phrase drop would let it
        // EVICT those clean names. Skip it so the individual pilots survive.
        if run_covered_by_pilots(&r, &pilots, systems) {
            continue;
        }
        // A loose blob that is just a CONFIRMED pilot name extended by a system/code token
        // ("Ruston Shackleford B-3QPD") is "name + location", not one held name. Don't add the
        // over-glued blob — it would only hold the system inside the name and (being longer)
        // evict the clean known name via the final sub-phrase drop. Instead surface the known
        // name; the freed code is then picked up by the location pass. This is the deterministic
        // equivalent of the ESI cover splitting the run (here the cache already knows the name).
        if let Some(known_name) = known_name_in_system_run(&r, known_pilots, systems) {
            if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&known_name)) {
                pilots.push(known_name);
            }
            continue;
        }
        pilots.push(r);
    }
    // A pasted killmail link ("Kill:/击杀： <Victim> (<Ship>)"): surface the victim whole + the hull.
    // Runs AFTER loose_pilot_runs so a Latin killword that glued onto the name as a run ("Kill Lord
    // Road") is present to drop; the clean victim then wins the sub-phrase pass, and the glued-tail
    // mis-parse ("Road") is dropped as a sub-phrase of the victim. Case is not consulted.
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
    // Single-word names queued for ESI. Case-INSENSITIVE: pilots are usually proper-case but
    // are sometimes typed all lower-case ("bigfoott"), and those must not be dropped — only a
    // stop-word or a recognised entity (ship/system/code/keyword) is excluded. Uses the
    // ship/paren-masked tokens so a multi-word hull's words aren't read as names.
    let masked_tokens = tokenize(&masked);
    for t in &masked_tokens {
        let lc = t.to_lowercase();
        let name_word = t.chars().count() >= 3
            && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '\'' || c == '-')
            && t.chars().any(|c| c.is_ascii_alphabetic());
        if name_word
            && !is_pilot_stopword(t)
            // A demoted-for-inactivity name (Phase 2) must not be re-proposed as a candidate
            // here either, or its token would never be freed for keyword/ship/system parsing.
            && !denied.contains(&lc)
            && !is_cap_word(&lc)
            && !is_tackle_word(&lc)
            && !is_time_token(t)
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
    // A structure name (Keepstar, Fortizar, …) is never a pilot, even if a character is
    // named after one — it's reported as a structure badge, not a player.
    pilots.retain(|p| !is_structure_word(p));
    // HINT (pasted chat links only): an in-game copy/paste separates each linked entity (pilot,
    // ship, system) with a double space, and a name/ship never contains one — so each
    // double-space segment is exactly one entity. Hand-typed text has no double spaces (handled
    // by the un-glue below), so this only augments: when present, drop any multi-word candidate
    // that straddles a boundary (a mis-glue of separate links) and surface each name-shaped
    // segment as a pilot. A segment is a name unless it is a system, a ship, or made up entirely
    // of lowercase stop words (names may otherwise be lowercase, e.g. "wenmg").
    // Names surfaced from a paste are confirmed linked entities, so they bypass the single-word
    // dictionary prose filter below (a real linked pilot named "fibular" is not prose).
    let mut paste_origin: std::collections::HashSet<String> = std::collections::HashSet::new();
    if text.contains("  ") {
        let segments: Vec<&str> =
            text.split("  ").map(str::trim).filter(|s| !s.is_empty()).collect();
        // Only treat it as a paste when EVERY segment is a clean single entity (system, ship, or
        // name) — a real link paste is, but prose with a stray double space ("rorqual  pointed in
        // Jita") isn't, so we fall back to normal logic there. Names to surface are collected.
        let names: Option<Vec<&str>> = (segments.len() > 1)
            .then(|| {
                let mut names = Vec::new();
                let mut anchor = false; // a system/ship/structure confirming this is intel, not chat
                for seg in &segments {
                    let lc = seg.to_lowercase();
                    // A clean paste segment is ONE entity. A segment carrying a decorated count
                    // ("+12") or built entirely of ship hulls ("kikis flycatcher kirin") is
                    // hand-typed intel, not a pasted name — bail to normal parsing so the count
                    // and each hull are detected instead of being glued into a pilot blob.
                    let seg_words: Vec<&str> = seg.split_whitespace().collect();
                    if (seg_words.len() > 1 && seg_words.first().is_some_and(|w| is_decorated_count(w)))
                        || (seg_words.len() > 1
                            && seg_words.iter().all(|w| ship_of(&w.to_lowercase(), ship_index).is_some()))
                    {
                        return None;
                    }
                    let is_system = (looks_like_system_code(seg)
                        && !is_code_lookalike_name(seg, systems))
                        || resolve(systems, seg).is_some();
                    let is_ship = ship_index.contains_key(&lc)
                        || is_structure_word(seg)
                        || crate::wormholes::is_wh_code(seg);
                    if is_system || is_ship {
                        anchor = true;
                        continue;
                    }
                    if segment_is_name(seg, systems) {
                        names.push(*seg);
                    } else {
                        return None; // a prose segment — not a paste
                    }
                }
                // Require an anchor so arbitrary double-spaced chat ("cats  love  fish") isn't
                // mistaken for a pilot paste; real intel pastes carry the location/hull.
                (anchor && !names.is_empty()).then_some(names)
            })
            .flatten();
        if let Some(names) = names {
            // Drop any multi-word candidate that straddles a boundary (a mis-glue of separate
            // links): it must fit inside a single segment.
            let seg_padded: Vec<String> =
                segments.iter().map(|s| format!(" {} ", s.to_lowercase())).collect();
            pilots.retain(|p| {
                !p.contains(' ')
                    || seg_padded.iter().any(|s| s.contains(&format!(" {} ", p.to_lowercase())))
            });
            for seg in names {
                // A paste segment may carry a typed location tail ("Garen Willow at taj"); surface
                // just the pasted name.
                let name = trim_paste_location_tail(seg, ship_index);
                paste_origin.insert(name.to_lowercase());
                if !pilots.iter().any(|p| p.eq_ignore_ascii_case(&name)) {
                    pilots.push(name);
                }
            }
        }
    }
    // A candidate that glues a name-tail to a hull ("Wilen Stabber" = the tail of "Elizabeth van
    // Wilen" + the ship "Stabber") is spurious when its non-ship remainder is ALREADY a sub-phrase
    // of a longer candidate. Strip a leading/trailing ship token and drop the candidate if the
    // remainder is so covered — the real name is the longer candidate and the hull is detected on
    // its own. Only drop when covered, so a genuine "Sabre Pilot" (whose "Pilot" isn't covered by a
    // longer name) is never touched, and no standalone name is lost.
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
    // two candidate spans PARTIALLY overlap (they share a word but neither contains the other —
    // "Lord Road" vs "Road he's", both claiming "Road"), keep the stronger name (a known/confirmed
    // one, else the longer, else the leftmost) and drop the other, so a tail a longer name already
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
        // The stronger of two overlapping candidates: known-confirmed wins, then more words, then
        // the leftmost start.
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
    // Final pass: drop any pilot that is a contiguous sub-phrase of a longer detected one.
    // The loose-run and single-token sources are added after the earlier sub-phrase filter,
    // so a short span the longer name already covers ("Chen Chen" inside "Dr Chen Chen",
    // produced because the loose run breaks on the 2-char "Dr") can slip through.
    drop_subphrase_pilots(&mut pilots, &std::collections::HashSet::new(), text);

    // Prose filter: a single LOWERCASE English word ("time", "carpet", lone "I") is prose, never a
    // roaming pilot — drop it even if a character shares the name or it was re-surfaced from a
    // system-adjacent / known-name run. Single-word only (multi-word lowercase names still go to
    // ESI), and any Capitalised token is a name (`is_lowercaseish` is false). Runs after every pilot
    // source so a later known-name re-add can't reintroduce it. A quoted word ("'clear'") is an
    // explicit pilot and is kept.
    pilots.retain(|p| {
        p.contains(' ')
            || !is_lowercaseish(p)
            || quoted.contains(&p.to_lowercase())
            || paste_origin.contains(&p.to_lowercase())
            || !crate::dict::is_word(p)
    });

    // A keyword inside a name suppresses the matching status flag ("The Bubble Boy" is not a
    // bubble), but a noise blob full of prose/keywords must NOT — else a real "camped"/"bubble"
    // gets silenced. So only a candidate anchored by a STRONG name word (mixed-case, capitalised,
    // not itself a stop word/keyword) contributes its tokens here.
    let is_strong_name_word = |w: &str| {
        name_part(w) && w.chars().any(|c| c.is_ascii_lowercase()) && !is_pilot_stopword(w)
    };
    let mut pilot_tokens: std::collections::HashSet<String> = pilots
        .iter()
        .filter(|n| n.split_whitespace().any(|w| is_strong_name_word(w)))
        .flat_map(|n| n.split_whitespace())
        .map(|w| w.to_lowercase())
        .collect();
    // Active pilots whose names embed intel keywords (matched case-sensitively): their tokens are
    // treated as a name, so the keyword inside can't spoof a status (e.g. a "cyno" alert).
    for name in KEYWORD_NAME_PILOTS {
        if display_text.contains(name) {
            pilot_tokens.extend(name.split_whitespace().map(|w| w.to_lowercase()));
        }
    }
    // Tokens INSIDE a multi-word pilot span — including an all-lower-case confirmed name with no
    // strong-name word ("bovine worm") that `pilot_tokens` (strong-name-filtered) misses. A hull
    // word inside such a span is part of the player's name, never a separate ship, so it's masked
    // from ship detection. A single-word pilot never masks a hull ("Bob in a Worm" keeps the Worm),
    // and an all-ship blob ("Sabre Orthrus") is still recovered as ships by the reclassify pass.
    let pilot_span_tokens: std::collections::HashSet<String> = pilots
        .iter()
        .filter(|n| n.split_whitespace().count() > 1)
        .flat_map(|n| n.split_whitespace())
        .map(|w| w.to_lowercase())
        .collect();

    // Wormhole signature code (e.g. "K162") named anywhere in the message.
    let wh_code =
        tokens.iter().find(|t| crate::wormholes::is_wh_code(t)).map(|t| t.to_uppercase());

    // Extra wormhole detail, parsed only for an actual wormhole sighting.
    let is_wh_msg = lower.contains("wormhole")
        || wh_code.is_some()
        || lower_tokens.iter().any(|t| {
            matches!(t.as_str(), "wh" | "hole" | "holes" | "thera" | "turnur")
                && !pilot_tokens.contains(t)
        });
    let (wh_dest, wh_eol, wh_drifter, wh_sig) = if is_wh_msg {
        (
            parse_wh_dest(&lower, &lower_tokens),
            lower.contains("eol") || lower.contains("end of life") || lower.contains("dying"),
            lower.contains("drifter"),
            tokens.iter().find(|t| looks_like_sig(t)).map(|t| t.to_uppercase()),
        )
    } else {
        (None, false, false, None)
    };

    // Ships: hull names / nicknames / acronyms (case-insensitive), or an unambiguous
    // typo. A token that belongs to a pilot name is never also parsed as a ship.
    let mut ships: Vec<DetectedShip> = Vec::new();
    let add_ship = |id: i64, name: &str, ships: &mut Vec<DetectedShip>| {
        if !ships.iter().any(|s| s.id == id) {
            ships.push(DetectedShip { id, name: name.to_owned() });
        }
    };
    // Words that belong to a detected multi-word hull ("Catalyst" in "Catalyst Navy
    // Issue") must not also be read as a standalone ship (double-counting the hull).
    let mw_words: std::collections::HashSet<String> = {
        let punct = |c: char| ",.;:!?\"()".contains(c);
        let tw: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(punct)).collect();
        let mut s = std::collections::HashSet::new();
        for (start, len, _, name) in &mw_ships {
            // The hull's canonical words ("Catalyst" in "Catalyst Navy Issue") AND the original
            // source tokens (which for a fuzzy match include the typo, "cythe") — so neither the
            // canonical word nor the typo is re-read as a standalone ship/pilot.
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
        // Resolution order: an exact system name or an exact known character is
        // never reinterpreted as a ship typo (a null-sec abbreviation is resolved as
        // a system in the systems pass, not here).
        if systems.lookup(tok).is_some() || known_pilots.contains_key(&lower) {
            continue;
        }
        // Typo tolerance applies to English/ASCII only; translated names are matched
        // exactly (above), never fuzzily.
        if lower.is_ascii() && lower.len() >= 5 {
            // Typo: accept only if exactly one ship name is within edit distance 1.
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
    // Ships named inside drag-drop parentheses (resolved by full name above).
    for (id, name) in drop_ships {
        add_ship(id, &name, &mut ships);
    }
    // Multi-word hull names ("Exequror Navy Issue") detected before pilot parsing.
    for (_, _, id, name) in mw_ships {
        add_ship(id, &name, &mut ships);
    }

    // Reserve EVERY candidate-name token (not the strong-name-filtered `pilot_tokens` used for
    // keyword suppression) so a system inside a lower-case name blob ("bob uitra") is held too.
    let name_tokens: std::collections::HashSet<String> =
        pilots.iter().flat_map(|p| p.split_whitespace()).map(|w| w.to_lowercase()).collect();
    let (mut detected, gates, mut consumed) = detect_location(
        &tokens, &lower_tokens, &name_tokens, systems, context_system, channel_regions,
    );
    // Known multi-word system names ("Sanctified Vidette") were masked out of pilot detection
    // above; add them as detected systems (their single tokens don't resolve alone, so
    // `detect_location` couldn't have). Their words are consumed so they aren't also read as a
    // hostile count or pilot.
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

    // NPC "diamond rats" and "anom/sig <code>" callouts: NPCs / anomaly ids, never pilots or
    // systems. Their words are consumed below so a "Diamond Rats" / "ABC-123" run can't surface
    // as a player name. The anom/sig code is gated against the real-systems lookup inside
    // `detect_anom_sigs`, so a code that is a real (or neighbouring) system stays a system.
    let (diamond_rats, dia_consumed) = detect_diamond_rats(&tokens);
    let (anom_sigs, anom_consumed) = detect_anom_sigs(&tokens, systems);
    let npc_consumed: std::collections::HashSet<String> =
        dia_consumed.into_iter().chain(anom_consumed).collect();
    consumed.extend(npc_consumed.iter().cloned());

    // Alliance shorthands ("frat", "init", …) → logos on the card.
    let mut alliances: Vec<(String, i64)> = Vec::new();
    for t in &lower_tokens {
        if let Some((name, id)) = crate::alliances::lookup(t) {
            if !alliances.iter().any(|(_, i)| *i == id) {
                alliances.push((name.to_owned(), id));
            }
        }
    }

    // A run whose every word is a known ship is ships, not a pilot ("Sabre Orthrus");
    // move it to the ship list (a coincidental same-named character loses).
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

    // RELIABILITY GUARANTEE: a full hull name that is NOT a confirmed pilot must never be
    // swallowed by a pilot-run/paste/+count heuristic. The general loop above skips any token
    // held inside a multi-word pilot candidate (`pilot_span_tokens`) — correct when the whole
    // blob is one real name, but WRONG when the blob mixes a CONFIRMED pilot with a leftover
    // hull ("Bob Rifter", Bob known → pilot Bob + ship Rifter): the hull would be lost. So for
    // every multi-word candidate that contains a confirmed-pilot word AND a leftover word that
    // EXACTLY matches a known hull (case-insensitive, via `ship_of`), surface that hull here.
    // The blob itself is still split into the confirmed name by the ESI cover downstream.
    // A hull that is genuinely PART of a confirmed name ("Wolf E Kristjansson", "bovine worm")
    // stays masked (all its words are confirmed); a fully-unconfirmed 2-word blob ("Sabre
    // Smith", neither word cached) is left to the cover unchanged — no forced ship there.
    {
        // Tokens that belong to a confirmed pilot: a candidate whose whole name is in the known
        // cache (or was quoted), plus any single word that is itself a known character.
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
            // Only fire when a confirmed pilot is present in the blob — that's the signal the
            // blob is "pilot + leftover ship", not one ambiguous 2-word name pending ESI.
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

    // Scanning probes (Core/Combat Scanner Probe items + slang) are reported as a badge,
    // never as the Probe frigate — drop the frigate so it isn't double-detected. First mask
    // case-sensitive keyword-named pilots ("RSS Scanner Probe") so a player's name doesn't
    // trigger a probe badge. Real probe items ("Sisters Core Scanner Probe") don't contain
    // those exact names, so legitimate detection is unaffected.
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

    let classes = detect_classes(&lower_tokens);
    let (mut tackled, tackled_targets) = detect_tackle(&lower_tokens, &pilot_tokens, ship_index);
    // Best-guess Chinese tackle/point/web terms (not seen in current logs — a safety net).
    tackled |= lower.contains("抓") || lower.contains("点住") || lower.contains("网住");

    // Celestial locations ("planet 1", "moon IV", "sun"): their word + number are consumed so
    // they aren't read as a hostile count or a pilot. Uses the raw split (tokenize drops bare
    // numbers, which are exactly the celestial index).
    let raw_tokens: Vec<&str> = text.split_whitespace().collect();
    let (mut celestials, celestial_consumed) = detect_celestials(&raw_tokens);
    consumed.extend(celestial_consumed);
    // Asteroid/ice belts (spans found before pilot detection) are celestial-style location
    // badges; consume their words so they aren't also read as a count or system.
    for (start, len, label) in &belt_spans {
        if !celestials.iter().any(|c| c.eq_ignore_ascii_case(label)) {
            celestials.push(label.clone());
        }
        for w in cel_words.iter().skip(*start).take(*len) {
            consumed.push(w.clone());
        }
    }

    let mut pilots = drop_covered_prefixes(&pilots, text);
    // A pilot name always contains a letter — a bare number ("warpin 100") is a count, not a name.
    pilots.retain(|p| p.chars().any(|c| c.is_alphabetic()));
    // Case and length don't decide a name: EVE names can be all-caps and short ("DT", "PORTOS11").
    // System codes are caught by `looks_like_system_code` / the neighbour-gate resolution and ship
    // acronyms by the ship index; anything else is left for ESI to confirm or reject.
    // A single token consumed as a system or gate — including a lower-case null-sec code
    // like "c-j" in "c-j gate" — is never also a pilot.
    pilots.retain(|p| p.contains(' ') || !consumed.contains(&p.to_lowercase()));
    // A code-shaped token resolved as a neighbour-gate abbreviation ("F2A") must also be stripped
    // from any name blob it gummed onto ("5 reds f2a" → "5 reds", "Bob f2a" → "Bob"); a blob left
    // empty is dropped. Only code-shaped consumed tokens are stripped, so a consumed system *name*
    // never carves a word out of a real pilot.
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
    // Dedupe (case-insensitive) so the same name repeated — in one message or across merged
    // re-posts — never inflates the hostile count ("X X X" is one hostile, not three).
    {
        let mut seen = std::collections::HashSet::new();
        pilots.retain(|p| seen.insert(p.to_lowercase()));
    }
    // Surface allow-listed keyword-named pilots (case-sensitive) if the heuristic missed them.
    for name in KEYWORD_NAME_PILOTS {
        if display_text.contains(name) && !pilots.iter().any(|p| p.eq_ignore_ascii_case(name)) {
            pilots.push((*name).to_string());
        }
    }
    // Strip NPC "rats"/anom-code words from any pilot blob they glued onto ("Diamond Rats",
    // "Anom ABC-123") and drop a name left empty — they're NPCs / anomaly ids, not players.
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
    // "pilot1 pilot2 +20" = 22; a stated total ("7 reds") wins; otherwise a bare list of
    // 3+ named pilots reports its own count. Fewer than 3 named with no number = no badge.
    let named = pilots.len() as u32;
    let count = if let Some(t) = total_count {
        Some((t + plus_count).min(999)) // a stated number ("3 Drake +2" = 5)
    } else if plus_count > 0 {
        Some((named + plus_count).min(999)) // named pilots + "N more" ("a b +20" = 22)
    } else if named >= 3 {
        Some(named) // a bare list of 3+ named pilots
    } else {
        None
    };
    let ess_ctx = lower_tokens.iter().any(|t| t == "ess" && !pilot_tokens.contains(t));
    let isk = parse_isk(text, ess_ctx);
    let structures = detect_structures(text);
    let mut report = IntelReport {
        id: 0, // assigned by IntelState::push
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
        name_number_skips,
        isk,
        structures,
        celestials,
        // Status keywords ignore words that belong to a pilot-name run, so a pilot
        // named e.g. "Clear Skies" can't spoof a "clear" status. A "?" makes it a question
        // ("clear?", "is it safe?"), not an assertion — never a clear.
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
        camp: flagged(&lower_tokens, &pilot_tokens, &["camp", "gatecamp", "camping", "camped", "gatecamping"]) || lower.contains("蹲"),
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
        killmail: links.iter().any(|l| l.kind == LinkKind::Killmail)
            || KILL_WORDS.iter().any(|w| lower.contains(w)),
        near_celestial: None, // filled by the zKill ingest path when a position is available
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

/// Parse a "time left" callout: "M:SS" (e.g. "5:30"), or a number followed by a
/// minute/second unit — attached ("5m", "30s") or spaced ("5 min", "30 sec").
/// Minutes over `max_min` are rejected (a big "Xm" is an ISK amount, e.g. "77m
/// bank", not a hack timer).
fn parse_time_left(text: &str, max_min: u32) -> Option<String> {
    let toks: Vec<&str> = text.split_whitespace().collect();
    // "M:SS" minutes:seconds.
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
    // A number with a minute/second unit (the unit may be the next token).
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

/// True if a token names a capital ship class.
fn is_cap_word(t: &str) -> bool {
    matches!(
        t,
        "cap" | "caps" | "capital" | "capitals" | "rorq" | "rorqs" | "rorqual" | "rorquals"
            | "dread" | "dreads" | "dreadnought" | "dreadnoughts" | "carrier" | "carriers"
            | "fax" | "faxes" | "titan" | "titans" | "super" | "supers" | "supercap"
            | "supercaps" | "supercarrier" | "supercarriers"
    )
}

/// True if a token is a tackle verb (prefix-matched for typo/tense robustness:
/// tackl*, takl*, scram*, scrambl*, point*).
fn is_tackle_word(t: &str) -> bool {
    t.starts_with("tackl")
        || t.starts_with("takl")
        || t.starts_with("tackel")
        || t.starts_with("scram")
        || t.starts_with("scrambl")
        || t.starts_with("point")
}

/// Any ship/type reported tackled. Returns whether a tackle word appears and the
/// ship/class names immediately preceding one (for the "<ship> TACKLED" badge).
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

/// A capital reported tackled: a cap-class word AND a tackle word both appear
/// anywhere in the message (robust to word order, spacing, and tense/typos). Words
/// that belong to a pilot name are ignored.
fn detect_cap_tackled(
    lower_tokens: &[String],
    pilot_tokens: &std::collections::HashSet<String>,
) -> bool {
    let cap = lower_tokens.iter().any(|t| !pilot_tokens.contains(t) && is_cap_word(t));
    let tackle = lower_tokens.iter().any(|t| !pilot_tokens.contains(t) && is_tackle_word(t));
    cap && tackle
}

/// True if any token starts with one of `stems` and is neither part of a pilot name
/// nor preceded by a negation ("no", "not", "without") — so "no bubble" doesn't set
/// `bubble`.
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

/// Like `flagged` but requires an exact token match, for short keywords whose prefix
/// collides with names/ships ("help" in "Helper", "cyno" in the ship "Cynabal").
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

/// "Title-Case + lower-case word + a number" runs — a pilot name even when the
/// first word is a system ("Amarr slave 3424"); the trailing number disambiguates.
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

/// A bare 1–2 digit number — ambiguous between a system/gate code and a count.
fn is_short_number(t: &str) -> bool {
    (1..=2).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit())
}

/// Resolve a token to a system: exact name, or an unambiguous null-sec abbreviation
/// (uppercase/digit code with a hyphen, e.g. "78-", "C-J"). The proper-noun guard
/// keeps common lower-case words from matching.
fn resolve<'a>(systems: &'a Systems, token: &str) -> Option<&'a crate::geo::SystemInfo> {
    let first = token.chars().next()?;
    let proper = first.is_uppercase() || first.is_ascii_digit() || token.contains('-');
    if !proper {
        return None;
    }
    if let Some(info) = systems.lookup(token) {
        return Some(info);
    }
    // A bare 2-digit number is only a system if it's a "<digits>-…" null-sec code
    // (e.g. "78" → 78-AAA), never an arbitrary prefix — safer, may drop some intel.
    if token.len() == 2 && token.chars().all(|c| c.is_ascii_digit()) {
        return systems.lookup_prefix(&format!("{token}-"));
    }
    // Null-sec codes: uppercase/digit/hyphen, e.g. "78-", "C-J".
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

/// EVE structures + their common in-game abbreviations (verified against player usage,
/// not invented). Single-word entries match whole tokens; two-word entries match phrases.
const STRUCTURES: &[(&str, &str)] = &[
    // Citadels
    ("keepstar", "Keepstar"), ("keep", "Keepstar"), ("ks", "Keepstar"),
    ("fortizar", "Fortizar"), ("fort", "Fortizar"),
    ("astrahus", "Astrahus"), ("astra", "Astrahus"),
    // Engineering complexes
    ("raitaru", "Raitaru"), ("azbel", "Azbel"), ("sotiyo", "Sotiyo"),
    // Refineries
    ("athanor", "Athanor"), ("tatara", "Tatara"),
    // Navigation / cyno
    ("ansiblex", "Ansiblex"), ("ansi", "Ansiblex"),
    ("tenebrex", "Cyno Jammer"), ("cyno jammer", "Cyno Jammer"),
    ("pharolux", "Cyno Beacon"), ("cyno beacon", "Cyno Beacon"),
    // Player-owned starbase (control tower)
    ("pos", "POS"),
    // Planetary / Equinox
    ("poco", "POCO"),
    ("skyhook", "Skyhook"),
    ("metenox", "Metenox"), ("moon drill", "Metenox"),
    ("mercenary den", "Mercenary Den"), ("merc den", "Mercenary Den"),
    ("sovereignty hub", "Sov Hub"), ("sov hub", "Sov Hub"),
];

/// Structure display name → its EVE type id, for showing the type image on intel cards and
/// resolving structure killmails (the SDE ship table doesn't carry Upwell structures).
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

/// The EVE type id for a structure display name (for its card image), if known.
pub fn structure_type_id(name: &str) -> Option<i64> {
    STRUCTURE_TYPES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, id)| *id)
}

/// The structure display name for an EVE type id (resolving structure killmails), if known.
pub fn structure_name_by_type(id: i64) -> Option<&'static str> {
    STRUCTURE_TYPES.iter().find(|(_, i)| *i == id).map(|(n, _)| *n)
}

/// Whether a single lower-case token names a structure (so it isn't read as a pilot).
fn is_structure_word(t: &str) -> bool {
    let lw = t.to_lowercase();
    STRUCTURES.iter().any(|(m, _)| !m.contains(' ') && *m == lw.as_str()) || is_skyhook_typo(&lw)
}

/// A near-miss spelling of "skyhook" ("skhook", "skyook", "skyhok") — edit distance <= 1
/// with a "sk" prefix so real names aren't swept in. Skyhooks dominate null-sec intel and
/// are routinely fat-fingered, so the typo should still raise the structure.
fn is_skyhook_typo(w: &str) -> bool {
    let w = w.to_lowercase();
    w.len() >= 5 && w.starts_with("sk") && crate::shipnames::edit_distance(&w, "skyhook") <= 1
}

/// A distance off a structure: "500km", "2au"/"2AU", or a bare number followed by
/// "off"/"km"/"au".
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

/// Asteroid/ice belt mentions as `(start, len, label)` over `structure_words` slots — a
/// celestial-style location badge ("Ice Belt", "Asteroid Belt", or a bare "Belt"). The
/// qualifier precedes the belt word, so a leading "ice"/"asteroid" is folded into the span.
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

/// Celestial locations named in intel: "planet"/"moon" + an arabic number or roman numeral
/// ("planet 1", "moon IV"), and a standalone "sun". Returns the display labels plus the
/// tokens consumed (the celestial word + its number) so they aren't read as a hostile count
/// or a pilot.
/// Roman numeral (I/V/X only, as used for planet indices) -> integer. 0 on an unexpected char.
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
        // A lone "I" collides with the pronoun ("moon I think it's clear"), so require a
        // real numeral of length >= 2 or a non-I single letter (V/X).
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
            // A plain number ("moon 3") or the planet-moon form ("moon 5-3", where the planet
            // number matters) — accept digits with an interior hyphen.
            if !n.is_empty()
                && n.starts_with(|c: char| c.is_ascii_digit())
                && n.chars().all(|c| c.is_ascii_digit() || c == '-')
            {
                let mut label = format!("{k} {n}");
                // A pasted moon location ("S-E6ES VI - Moon 12") has the planet roman just before
                // "Moon"; fold it in so the planet number isn't lost -> "Moon 6-12".
                if k == "Moon" && !n.contains('-') {
                    let mut j = i;
                    while j > 0 {
                        j -= 1;
                        let t = tokens[j].trim_matches(|c: char| !c.is_ascii_alphanumeric());
                        if t.is_empty() {
                            continue; // a bare "-" separator
                        }
                        if is_roman(t) {
                            label = format!("Moon {}-{n}", roman_value(t));
                            consumed.push(t.to_lowercase());
                        }
                        break; // only the immediately-preceding non-separator token counts
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

/// Scanning probes — Core/Combat Scanner Probe items (incl. Sisters/RSS/Satori-Horigu) and
/// the "core/combat probes" slang — as a badge label, distinct from the Probe frigate. A
/// lone "probe" (no Core/Combat/scanner qualifier) is the ship, so returns None.
fn detect_probes(text: &str) -> Option<Probes> {
    let lower = text.to_lowercase();
    // Match the "prob" stem so abbreviations like "combat prob" count too.
    let core = lower.contains("core scanner") || lower.contains("core prob");
    let combat = lower.contains("combat scanner") || lower.contains("combat prob");
    match (core, combat) {
        (true, false) => Some(Probes::Core),
        (false, true) => Some(Probes::Combat),
        (true, true) => Some(Probes::Any),
        (false, false) => {
            // A bare "prob" is shorthand for "probably", not scanning probes — only the
            // unambiguous "probes" (or a qualified "scanner/core/combat prob") counts.
            let bare = lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| matches!(w, "probes" | "probs"));
            (lower.contains("scanner prob") || bare).then_some(Probes::Any)
        }
    }
}

/// Lower-cased, punctuation-trimmed words of a message, aligned 1:1 with
/// `text.split_whitespace()` so spans returned here can mask the same word slots
/// elsewhere (e.g. before pilot detection).
fn structure_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.').to_lowercase())
        .collect()
}

/// Matched structure spans as `(start, len, canonical name)` over `structure_words`
/// positions. Shared by `detect_structures` and the pilot-masking pass so a structure
/// like "Cyno Beacon" is never also read as a pilot ("Beacon").
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
            // A single-word skyhook typo ("skhook") still raises the Skyhook structure.
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

/// Structures mentioned in the message, each with an optional distance off it
/// ("Keepstar 500km", "Astrahus 2AU").
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

/// Compact ISK display ("300M", "1.5B", "750K").
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

/// Parse an approximate count: `+5`, `x4`, `4x`, or a bare small number. A `+`/`x`
/// decorated number is always a count; a bare number is a count only if it wasn't
/// consumed as a system/gate (so "78" in "on 78 gate" isn't 78 hostiles).
fn parse_count(
    text: &str,
    consumed: &[String],
    systems: &Systems,
    ship_index: &HashMap<String, (i64, String)>,
    pilots: &[String],
    known_pilots: &HashMap<String, i64>,
) -> (Option<u32>, u32, Vec<(String, u32)>) {
    let mut name_skips: Vec<(String, u32)> = Vec::new();
    // A bare number directly before one of these is an ISK/quantity amount ("334
    // million"), not a hostile count.
    // Mirror parse_isk's suffixes so a spaced amount ("300 kk", "5 bill") isn't a count.
    const MAGNITUDE: &[&str] = &[
        "m", "mil", "mill", "million", "millions", "mio", "kk", "b", "bil", "bill", "billion",
        "billions", "k", "isk",
    ];
    let mut best: Option<u32> = None;
    let mut plus: u32 = 0;
    let words: Vec<&str> = text.split_whitespace().collect();
    for (i, raw) in words.iter().enumerate() {
        // Skip system codes (e.g. "78-", "1DQ1-A") — their digits aren't a count.
        if raw.contains('-') {
            continue;
        }
        // Lower-case so an upper-case multiplier ("X5") decorates a count just like "x5".
        let t = raw
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '+' && c != 'x' && c != 'X')
            .to_ascii_lowercase();
        let t = t.as_str();
        let digits = t.trim_start_matches(['+', 'x']).trim_end_matches(['x', '+']);
        if digits.is_empty() || digits.len() > 3 {
            continue;
        }
        // "+9", "9+", "x9", "9x" all decorate a count.
        let decorated =
            t.starts_with('+') || t.starts_with('x') || t.ends_with('x') || t.ends_with('+');
        let bare_number = t.chars().all(|c| c.is_ascii_digit());
        if !(decorated || bare_number) {
            continue;
        }
        // A bare number that LEADS a CONFIRMED (known-cache) pilot name ("1 Tap Machine") is
        // part of that name, never a hostile count — the leading-digit mirror of the trailing-
        // number guard below. Keyed on the known cache (which begins with the same digit), so a
        // glued blob ("1 Tap Machine ENI") is handled like the clean mention. A genuine count
        // ("3 Drake" — not a confirmed pilot) and a keyword blob ("7 red 1 neut") are untouched.
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
        // A *bare* number right after a pilot name is part of the name ("Adama 80",
        // "Malcolm 41"), not a count. Decorated "+2"/"x2" is always a count, and a number
        // after a *system* ("Rancer 80") still counts.
        if bare_number && i > 0 {
            let prev = words[i - 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-');
            let plc = prev.to_lowercase();
            if name_part(prev) && systems.lookup(prev).is_none() && !ship_index.contains_key(&plc) {
                if let Ok(n) = digits.parse::<u32>() {
                    name_skips.push((format!("{prev} {digits}"), n));
                }
                continue;
            }
        }
        if bare_number && !decorated {
            // A bare number consumed as a system/gate is not a count.
            if consumed.iter().any(|c| c == &t.to_lowercase()) {
                continue;
            }
            // A bare number before a magnitude word is an ISK amount, not a count.
            if let Some(next) = words.get(i + 1) {
                let n = next.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
                if MAGNITUDE.contains(&n.as_str()) {
                    continue;
                }
            }
        }
        if let Ok(n) = digits.parse::<u32>() {
            if (1..=999).contains(&n) {
                // A leading/trailing "+" means N *more* besides the named pilots; keep it
                // separate so the caller can add it to the pilot count. Other tokens are a
                // stated total (summed: "7 red; 1 neut" -> 8).
                if t.starts_with('+') || t.ends_with('+') {
                    plus = (plus + n).min(999);
                } else {
                    best = Some(best.map_or(n, |b| (b + n).min(999)));
                }
            }
        }
    }
    (best, plus, name_skips)
}

/// Split into candidate tokens, keeping `-` and `'` (used in system/char names).
pub(crate) fn tokenize(text: &str) -> Vec<&str> {
    // Apostrophes are kept (O'Brien), but a leading/trailing one is stray punctuation
    // ("PeshyHod'" is the character "PeshyHod"), so trim it off the token ends.
    text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\''))
        .map(|t| t.trim_matches('\''))
        .filter(|t| t.len() >= 2)
        .collect()
}

/// Destination class guessed from a wormhole message's text. Only a guess — the
/// wormhole *type's* own class (and EVE-Scout data) override it. None if no class
/// keyword is present.
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

/// An EVE cosmic-signature id, "ABC-123".
fn looks_like_sig(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 7
        && b[3] == b'-'
        && b[..3].iter().all(u8::is_ascii_alphabetic)
        && b[4..].iter().all(u8::is_ascii_digit)
}

/// Parse an EVE timestamp ("2026.06.22 18:30:45", UTC) to unix seconds.
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
        // Same pilot in 3 distinct systems within the last hour (case-insensitive name).
        s.record("Bob", 30000001, now - 100);
        s.record("bob", 30000002, now - 200);
        s.record("BOB", 30000003, now - 300);
        // A repeat of a system shouldn't inflate the distinct count.
        s.record("bob", 30000001, now - 50);
        // An old sighting well outside the 4h window.
        s.record("bob", 30000099, now - 20000);

        assert_eq!(s.distinct_systems_since("bob", 3600, now), 3);
        assert!(s.revived("bob", now)); // 3+ in the last hour

        // The 4h window already excludes the old (now-20000) sighting.
        assert_eq!(s.distinct_systems_since("bob", SIGHTINGS_WINDOW, now), 3);
        // A window wider than the retention still sees the old sighting — until prune drops it.
        assert_eq!(s.distinct_systems_since("bob", 999_999, now), 4);
        s.prune(now);
        assert_eq!(s.distinct_systems_since("bob", 999_999, now), 3);

        // An unknown pilot → zero, not revived.
        assert_eq!(s.distinct_systems_since("nobody", 3600, now), 0);
        assert!(!s.revived("nobody", now));

        // Only 2 distinct in the last hour but 5 across 4h → revived via the wider window.
        let mut w = Sightings::default();
        for (i, dt) in [(1, 100), (2, 200), (3, 5000), (4, 6000), (5, 7000)] {
            w.record("roamer", 30000000 + i, now - dt);
        }
        assert_eq!(w.distinct_systems_since("roamer", 3600, now), 2);
        assert_eq!(w.distinct_systems_since("roamer", SIGHTINGS_WINDOW, now), 5);
        assert!(w.revived("roamer", now));

        // system_id <= 0 is ignored.
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

    /// Simulate the live pipeline's ESI resolution of candidate blobs: treat every name in
    /// `reals` as a confirmed character and every other 1–3 word span as a confirmed non-name,
    /// then claim the real characters out of each candidate the way the app's reconcile does
    /// (longest match first, via `PilotCache::cover`). Returns the displayed pilot names.
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
            // Every individual word too — name_windows skips <3-char spans ("I", "he"), but the
            // cover needs a definite verdict for each or it blocks waiting on a "pending" span.
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

    /// Emulate the live reconcile: resolve a report's candidate blobs against `reals`, reserve
    /// the confirmed names' tokens, then re-derive the location from the unreserved tokens.
    /// Returns (pilots, system names, gates) as the card would show them after ESI answers.
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

    /// Apply [`resolve_report`] in place — mirrors the live reconcile updating a report once ESI
    /// answers (confirmed pilots, re-derived systems/gates).
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
        // Baseline: with no denial, the confirmed "Comet" is anchored as a pilot.
        let base = analyze_ctx(
            "Comet tackled in Rancer", &s, &noships(), &known, 1, "ch", "x", None, &[], &empty,
        );
        assert!(
            base.pilots.iter().any(|p| p.eq_ignore_ascii_case("comet")),
            "baseline anchors Comet: {:?}",
            base.pilots
        );
        // Demoted (denied): "Comet" is NOT a pilot, and its freed token leaves the tackle
        // keyword and the system intact.
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
        // Unit behaviour: a multi-word string made up ENTIRELY of stop words is rejected as a
        // whole, while a single non-stop word anywhere keeps the candidate. Single-word behaviour
        // is unchanged (a lone stop word is still true; a lone real name is still false).
        assert!(is_pilot_stopword("they are"));
        assert!(is_pilot_stopword("back to"));
        assert!(is_pilot_stopword("still here"));
        assert!(is_pilot_stopword("they")); // single stop word unchanged
        assert!(!is_pilot_stopword("bob")); // single non-stop word unchanged
        assert!(!is_pilot_stopword("Navy Bob")); // one non-stop word keeps the run
        // Contractions (apostrophe and case insensitive).
        assert!(is_pilot_stopword("I'm"));
        assert!(is_pilot_stopword("im"));
        assert!(is_pilot_stopword("they're"));
        assert!(is_pilot_stopword("don't"));
        // A real name with an apostrophe is NOT a stop word.
        assert!(!is_pilot_stopword("O'Brien"));
        // "full" is intel prose ("full fleet", "bubble is full"), never a pilot — case-insensitive.
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

        // "I'm" is a contraction, never a pilot; the tackle keyword + system still parse.
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

        // A multi-word name (no stop words) survives.
        let r = a("Bob Hope tackled in Rancer");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Bob Hope")),
            "multi-word name survives: {:?}",
            r.pilots
        );
        // A distinctive single-word name survives.
        let r = a("I-Pustelga tackled in Rancer");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("I-Pustelga")),
            "distinctive single-word name survives: {:?}",
            r.pilots
        );

        // An all-stop run is dropped but a run with ONE non-stop word ("Navy Bob") survives.
        let r = a("Navy Bob tackled in Rancer");
        assert!(
            r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Navy Bob")),
            "one non-stop word keeps the run: {:?}",
            r.pilots
        );
    }

    // Rancer (1) gate-adjacent to F2A-3X (100); used for neighbour-abbreviation tests.
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
        // Channel's last-known system is Rancer (1); "F2A"/"f2a" is the abbreviation of its
        // neighbour F2A-3X — a gate callout to the adjacent system, not a pilot.
        // Bare "F2A"/"f2a" (new block) plus a hyphenated prefix "f2a-3" (the existing null-sec
        // code path) — null-sec coded systems carry exactly one hyphen, so a prefix may include it.
        for msg in ["Bob f2a", "hostiles F2A", "hostiles f2a-3"] {
            let r = analyze_ctx(msg, &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
            let on_f2a = r.gates.iter().any(|g| g.eq_ignore_ascii_case("F2A-3X"))
                || r.systems.iter().any(|d| d.name == "F2A-3X");
            assert!(on_f2a, "{msg}: F2A not resolved — gates={:?} systems={:?}", r.gates, r.systems.iter().map(|d| &d.name).collect::<Vec<_>>());
            assert!(!r.pilots.iter().any(|p| p.to_lowercase().contains("f2a")), "{msg}: F2A as pilot {:?}", r.pilots);
        }
        // The flanking name word survives — only the code is carved out of the blob.
        let r = analyze_ctx("Bob f2a", &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
        assert!(r.pilots.iter().any(|p| p == "Bob"), "Bob lost: {:?}", r.pilots);
    }

    #[test]
    fn surname_that_is_a_system_is_not_a_gate() {
        let s = systems();
        // "alexpanda Uitra" is a pilot; "Uitra" must not become a bogus gate, and N3-JBX is the
        // location — re-derived once ESI confirms the name and frees its token (held model).
        let r2 = analyze("N3-JBX* alexpanda Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
        let (pilots, sysd, gates) = resolve_report(&r2, &["alexpanda Uitra"], &s);
        assert_eq!(pilots, vec!["alexpanda Uitra".to_string()]);
        assert_eq!(sysd, vec!["N3-JBX".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
        // Title-case first name + system surname is also a pilot, not a gate.
        let r3 = analyze("N3-JBX Bob Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
        let (pilots, sysd, gates) = resolve_report(&r3, &["Bob Uitra"], &s);
        assert_eq!(pilots, vec!["Bob Uitra".to_string()]);
        assert_eq!(sysd, vec!["N3-JBX".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
        // But a genuine two-system mention (no name word) still yields a gate.
        let r4 = analyze("N3-JBX Uitra", &s, &noships(), &noknown(), 1, "ch", "AnewSs");
        let (pilots, sysd, gates) = resolve_report(&r4, &[], &s);
        assert!(pilots.is_empty(), "pilots={pilots:?}");
        assert_eq!(sysd, vec!["N3-JBX".to_string()]);
        assert!(gates.iter().any(|g| g == "Uitra"), "gates={gates:?}");
    }

    /// "alexpanda Uitra" is a confirmed character whose surname happens to be a real system
    /// (Uitra). The system word inside the confirmed name must NOT be pulled out as a gate or a
    /// standalone system — even standing alone (no corroborating second system) and even when
    /// followed by the "gate" keyword. A genuine lone system ("Uitra") still parses as a system,
    /// and a real "<system> gate" callout with no name word still yields the gate.
    #[test]
    fn confirmed_name_system_surname_not_pulled_as_gate() {
        let s = systems(); // includes Uitra (a real system) + Rancer
        // Alone: pilot only; Uitra is neither a system nor a gate once the name is confirmed.
        for line in ["alexpanda Uitra", "alexpanda Uitra gate", "alexpanda Uitra tackled"] {
            let r = analyze(line, &s, &noships(), &noknown(), 1, "ch", "x");
            // Proposed whole at raw parse (so ESI can confirm the full span).
            assert!(proposed(&r.pilots, "alexpanda Uitra"), "{line:?}: not proposed: {:?}", r.pilots);
            let (pilots, sysd, gates) = resolve_report(&r, &["alexpanda Uitra"], &s);
            assert_eq!(pilots, vec!["alexpanda Uitra".to_string()], "{line:?}: pilots={pilots:?}");
            assert!(sysd.is_empty(), "{line:?}: Uitra leaked as a system: {sysd:?}");
            assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "{line:?}: Uitra leaked as a gate: {gates:?}");
        }
        // With a genuine location present, that system wins and Uitra (the surname) is still held.
        let r = analyze("Rancer alexpanda Uitra gate", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, sysd, gates) = resolve_report(&r, &["alexpanda Uitra"], &s);
        assert_eq!(pilots, vec!["alexpanda Uitra".to_string()], "pilots={pilots:?}");
        assert!(sysd.iter().any(|n| n == "Rancer"), "Rancer missing: {sysd:?}");
        assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "Uitra leaked as a gate: {gates:?}");
        // Control: a real "<system> gate" with no name word still detects the gate.
        let r = analyze("N3-JBX Uitra gate", &s, &noships(), &noknown(), 1, "ch", "x");
        let (_p, _sysd, gates) = resolve_report(&r, &[], &s);
        assert!(gates.iter().any(|g| g.eq_ignore_ascii_case("Uitra")), "genuine Uitra gate lost: {gates:?}");
    }

    /// True when some pilot candidate contains `name` as a contiguous, case-insensitive run of
    /// whole words — i.e. the name was PROPOSED (possibly inside an over-glued blob the ESI cover
    /// later splits), regardless of whether it stands as its own entry yet.
    fn proposed(pilots: &[String], name: &str) -> bool {
        let want: Vec<String> = name.split_whitespace().map(|w| w.to_lowercase()).collect();
        pilots.iter().any(|p| {
            let ws: Vec<String> = p.split_whitespace().map(|w| w.to_lowercase()).collect();
            ws.windows(want.len()).any(|w| w == want.as_slice())
        })
    }

    /// True when `tok` survives as a word in any pilot candidate (a junk token must NOT).
    fn has_pilot_token(pilots: &[String], tok: &str) -> bool {
        pilots.iter().any(|p| p.split_whitespace().any(|w| w.eq_ignore_ascii_case(tok)))
    }

    #[test]
    fn stray_letter_midrun_splits_pilot_list() {
        let s = systems();
        // A relayed kill-list with a mistyped lone "v" wedged between names, and two Chinese ship
        // names carrying a "*" marker. willlin/qiuxiaoye are lower-case, so only the CONFIRMED
        // cache finds them; the capitalised names ride the loose runs. The lone "v" must NOT glue
        // "Micahel wu" onto the tail and swallow "Htguuu"/"Htg-0".
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
        // The lone letter is dropped, never a pilot or a token inside one.
        assert!(!has_pilot_token(&r.pilots, "v"), "stray v leaked: {:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p == "v"));
        // The hyphen-with-digit name stays intact.
        assert!(proposed(&r.pilots, "Htg-0"), "Htg-0 mangled: {:?}", r.pilots);
        // The Chinese ship tokens don't break the line and aren't read as pilots.
        assert!(!has_pilot_token(&r.pilots, "灵感级"), "ship as pilot: {:?}", r.pilots);
    }

    #[test]
    fn stray_word_midrun_splits_pilot_list() {
        let s = systems();
        // A single capitalised name on each side of a lone "v": split so BOTH are proposed
        // independently (each surfaces as its own single-token candidate), and "v" is dropped.
        let r = analyze("Alpha v Bravo", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Alpha"), "Alpha lost: {:?}", r.pilots);
        assert!(r.pilots.iter().any(|p| p == "Bravo"), "Bravo lost: {:?}", r.pilots);
        assert!(!has_pilot_token(&r.pilots, "v"), "stray v leaked: {:?}", r.pilots);

        // A stray chatter WORD ("lol") between real names is junk, not part of either name: it's
        // dropped and the names on both sides are still proposed.
        let r2 = analyze("Alpha Bravo lol Charlie", &s, &noships(), &noknown(), 1, "ch", "x");
        for name in ["Alpha", "Bravo", "Charlie"] {
            assert!(proposed(&r2.pilots, name), "{name:?} not proposed: {:?}", r2.pilots);
        }
        assert!(!has_pilot_token(&r2.pilots, "lol"), "stray lol leaked: {:?}", r2.pilots);

        // Conservative: an embedded stop word flanked by a name and a non-strong word (a keyword
        // that ends a real name) stays whole — "is" is NOT carved out of "Cult is Dead".
        let r3 = loose_pilot_runs("Cult is Dead", &noships(), &s);
        assert!(r3.iter().any(|p| p == "Cult is Dead"), "Cult is Dead split: {r3:?}");
    }

    #[test]
    fn stray_letter_before_name_with_code_system() {
        let s = systems();
        // Body of a relayed report: a stray single letter "v" the reporter typed, a 2-word pilot,
        // and a null-sec CODE system. "Ruston Shackleford" must be the pilot, B-3QPD the location.
        //
        // Known-cache path (deterministic): "Ruston Shackleford" was confirmed on a prior
        // sighting, so the watcher feeds it in `known`. The over-glued held blob
        // "Ruston Shackleford B-3QPD" must NOT win — the known name stands and the code frees as
        // the system, with no ESI round-trip needed.
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

        // First-sighting path (ESI-dependent held model): with no cache the report is parked with
        // the system held inside the name blob; once ESI confirms the name the reconcile splits it
        // and re-derives the location. Mirrors the live reconcile via `resolve_report`.
        let r = analyze("v Ruston Shackleford B-3QPD", &s, &noships(), &noknown(), 1, "ch", "Ixen Orlenard");
        let (pilots, sysd, gates) = resolve_report(&r, &["Ruston Shackleford"], &s);
        assert_eq!(pilots, vec!["Ruston Shackleford".to_string()], "raw pilots={:?}", r.pilots);
        assert_eq!(sysd, vec!["B-3QPD".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
    }

    #[test]
    fn held_model_lowercase_name_with_system() {
        let s = systems();
        // A lower-case name whose surname is a system ("bob uitra"): "uitra" is held inside the
        // name until ESI, and C-J6MT (also adjacent to the name) is held too — so at parse time
        // the report has NO location and is parked.
        let r = analyze("C-J6MT bob uitra", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.systems.is_empty(), "location must be held: {:?}", r.systems);
        assert!(has_held_system(&r, &s), "report should be parked");
        // ESI confirms "bob uitra": it's the pilot, C-J6MT is re-derived as the location, and
        // "Uitra" never becomes a bogus gate.
        let (pilots, sysd, gates) = resolve_report(&r, &["bob uitra"], &s);
        assert_eq!(pilots, vec!["bob uitra".to_string()]);
        assert_eq!(sysd, vec!["C-J6MT".to_string()]);
        assert!(gates.is_empty(), "gates={gates:?}");
        // If ESI says "bob uitra" is NOT a character, nothing is a pilot and the location stands.
        let (pilots, sysd, _) = resolve_report(&r, &[], &s);
        assert!(pilots.is_empty(), "pilots={pilots:?}");
        assert!(sysd.iter().any(|n| n == "C-J6MT"), "systems={sysd:?}");
    }

    #[test]
    fn fly_catcher_is_the_flycatcher_hull() {
        let s = systems();
        // "fly catcher" (spaced) is the Flycatcher interdictor, matched case-insensitively as a
        // two-word phrase — not two pilots "Fly"/"Catcher".
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
        // "<X> gate" proves X is a location: "on the IAS gate" is the gate to IAS-X, not a
        // pilot "IAS" (which the title-case/distinctive passes would otherwise add).
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
        // Case and length don't disqualify a name: a long all-caps name resolves...
        let r = analyze("C-J6MT  PORTOS11", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(esi_resolve(&r.pilots, &["PORTOS11"]), vec!["PORTOS11".to_string()]);
        // ...and a short all-caps name does too (ESI confirms it).
        let r2 = analyze("XEN in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(esi_resolve(&r2.pilots, &["XEN"]), vec!["XEN".to_string()]);
    }

    #[test]
    fn safe_is_a_clear_and_question_mark_suppresses_it() {
        let s = systems();
        // "safe" is equivalent to "clear" (when not part of a pilot name).
        assert!(analyze("Rancer safe", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        // A "?" makes it a question, never a clear.
        assert!(!analyze("Rancer clear?", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(!analyze("is Rancer safe?", &s, &noships(), &noknown(), 1, "ch", "x").clear);
    }

    #[test]
    fn paste_segment_is_not_unglued_by_the_cache() {
        let s = systems();
        let mut known = noknown();
        known.insert("ghost".into(), 1);
        known.insert("magician".into(), 2);
        // Nothing is un-glued pre-ESI: a candidate stays one name until ESI confirms the WHOLE
        // isn't a character. So a 2-word name whose words are separately cached stays whole...
        let paste = analyze("C-J6MT  Ghost Magician", &s, &noships(), &known, 1, "ch", "x");
        assert_eq!(paste.pilots, vec!["Ghost Magician".to_string()], "{:?}", paste.pilots);
        let typed = analyze("Ghost Magician in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert_eq!(typed.pilots, vec!["Ghost Magician".to_string()], "{:?}", typed.pilots);
        // ...and a 3-word run also stays whole at parse time — only once ESI rejects the whole and
        // confirms the handles does the reconcile (cover) split it into the list.
        let mut k3 = known.clone();
        k3.insert("gliar".into(), 3);
        k3.insert("mliarvis".into(), 4);
        k3.insert("sliarhia".into(), 5);
        let list = analyze("Gliar Mliarvis Sliarhia in Rancer", &s, &noships(), &k3, 1, "ch", "x");
        assert_eq!(list.pilots.len(), 1, "kept whole at parse time: {:?}", list.pilots);
        let split = esi_resolve(&list.pilots, &["Gliar", "Mliarvis", "Sliarhia"]);
        assert_eq!(split.len(), 3, "ESI-rejected whole + confirmed handles → list: {:?}", split);
        // A 2-word whole stays whole through ESI too (the handles aren't surfaced).
        assert!(esi_resolve(&paste.pilots, &["Ghost", "Magician"]).is_empty());
    }

    #[test]
    fn paste_segment_drops_typed_location_tail() {
        let s = systems();
        // A double-space paste of "C-J6MT  Garen Willow" with a typed " at <loc>" tail: surface the
        // pasted name, not "Garen Willow at taj".
        let r = analyze("C-J6MT  Garen Willow at taj", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["Garen Willow".to_string()], "pilots={:?}", r.pilots);
        assert!(r.systems.iter().any(|d| d.name == "C-J6MT"));
        // A 1-word-prefix name that legitimately contains a preposition is NOT truncated.
        assert_eq!(trim_paste_location_tail("Man in Black", &ships_with(&[])), "Man in Black");
        assert_eq!(trim_paste_location_tail("Lord of War", &ships_with(&[])), "Lord of War");
        assert_eq!(trim_paste_location_tail("Garen Willow at taj", &ships_with(&[])), "Garen Willow");
    }

    #[test]
    fn paste_segment_drops_trailing_count() {
        let s = systems();
        // "F3-8X2  01XcerberusX01 +3": the pasted name keeps its digits, the "+3" is the count,
        // not part of the name. (Real copy: two showinfo url tags, double-spaced.)
        let r = analyze("C-J6MT  01XcerberusX01 +3", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["01XcerberusX01".to_string()], "pilots={:?}", r.pilots);
        assert_eq!(r.count, Some(4), "count={:?}", r.count); // the named pilot + 3 more
        // A bare trailing number ("Malcolm 41") is ambiguous and is NOT trimmed.
        assert_eq!(trim_paste_location_tail("Malcolm 41", &ships_with(&[])), "Malcolm 41");
        assert_eq!(trim_paste_location_tail("01XcerberusX01 +3", &ships_with(&[])), "01XcerberusX01");
        assert_eq!(trim_paste_location_tail("Drake x4", &ships_with(&[])), "Drake");
    }

    #[test]
    fn pasted_urls_are_not_parsed_as_pilots() {
        let s = systems();
        // A dscan link + system: the URL is captured as a link, never read as a pilot, and the
        // system still resolves.
        let r = analyze("https://dscan.info/v/a626d009ffc3  Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.is_empty(), "url leaked as pilots: {:?}", r.pilots);
        assert_eq!(r.links.len(), 1, "dscan link should be captured");
        assert!(r.systems.iter().any(|d| d.name == "Rancer"));
        // A URL mid-sentence: its host/path fragments ("example", "Foo-Bar") are not pilots, but a
        // real adjacent name survives.
        let r2 = analyze("Bob https://example.com/Foo-Bar in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r2.pilots.iter().any(|p| p.to_lowercase().contains("foo") || p.contains("example") || p.contains("http")), "url fragments leaked: {:?}", r2.pilots);
        assert!(r2.pilots.iter().any(|p| p == "Bob"), "real name dropped: {:?}", r2.pilots);
    }

    #[test]
    fn belt_is_a_location_badge_not_a_pilot() {
        let s = systems();
        // "Ice Belt" / "Asteroid Belt" / a bare "Belt" are celestial location badges, never
        // pilots — even when title-cased.
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
        // Plain-text relay with the full name cached.
        let mut known2 = noknown();
        known2.insert("ji wuming".into(), 2112339969);
        known2.insert("wuming".into(), 999);
        let r2 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known2, 1, "ch", "x");
        assert_eq!(r2.pilots, vec!["ji wuming".to_string()], "got {:?}", r2.pilots);
        // Plain-text relay where only the SURNAME is cached: the leading lower-cased word is
        // grabbed so the name isn't truncated to "wuming".
        let mut known3 = noknown();
        known3.insert("wuming".into(), 999);
        let r3 = analyze("ji wuming  EIMJ-M", &s, &noships(), &known3, 1, "ch", "x");
        assert_eq!(r3.pilots, vec!["ji wuming".to_string()], "got {:?}", r3.pilots);
    }

    #[test]
    fn full_name_not_split_into_ship_and_pilot() {
        let s = systems();
        // "Wolf E Kristjansson" must stay one pilot — never "Wolf" (the assault frigate) +
        // "Kristjansson". A plain-text relay matches it whole via the known cache.
        let mut ships = noships();
        ships.insert("wolf".into(), (11371, "Wolf".into()));
        // Plain-text relay with the full name already known resolves whole, no "Wolf" ship.
        let mut known = noknown();
        known.insert("wolf e kristjansson".into(), 2122822665);
        let r2 = analyze("Wolf E Kristjansson nv", &s, &ships, &known, 1, "ch", "x");
        assert_eq!(r2.pilots, vec!["Wolf E Kristjansson".to_string()]);
        assert!(r2.ships.is_empty(), "ships={:?}", r2.ships);
    }

    #[test]
    fn rest_keyword_not_a_pilot_even_if_known() {
        let s = systems();
        // "rest" is a status word ("1 jackdaw, rest NV"), never a pilot — even when the
        // persisted cache has a real character named "Rest".
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
        // Even when the cache holds real players spelled like chat words, a lower-cased
        // mention is the word/abbreviation, not the pilot.
        let mut known = noknown();
        for (w, id) in [("sry", 1i64), ("gg", 2), ("ez", 3), ("neo", 4)] {
            known.insert(w.into(), id);
        }
        let r = analyze("sry gg ez that was ez in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
        // A capitalised short token IS still taken as a deliberate pilot mention.
        let r2 = analyze("Neo tackled in Jita", &s, &noships(), &known, 1, "ch", "Anaz");
        assert!(r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("neo")), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn name_with_bubble_keyword_is_a_pilot_not_a_bubble() {
        let mut by_name = std::collections::HashMap::new();
        by_name.insert("r0-dmm".to_string(), SystemInfo { id: 30000563, name: "R0-DMM".into(),
            security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
        let s = Systems::new(by_name, HashMap::new());
        // "The Bubble Boy" embeds the "bubble" keyword; the full name must reach the cover and
        // the warp-bubble flag must NOT fire.
        let r = analyze("R0-DMM  The Bubble Boy", &s, &noships(), &noknown(), 1, "ch", "Anniken");
        assert_eq!(r.pilots, vec!["The Bubble Boy".to_string()]);
        assert!(!r.bubble);
        // A real bubble call still fires.
        assert!(analyze("bubble up on gate R0-DMM", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
        // The destroyer "Dragoon" must NOT trip the bubble flag (the old "drag" prefix did).
        assert!(!analyze("2 Dragoons on gate R0-DMM", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
        // A drag-bubble call still fires.
        assert!(analyze("drag bubble on the R0-DMM gate", &s, &noships(), &noknown(), 1, "ch", "x").bubble);
    }

    #[test]
    fn standing_color_led_name_reaches_the_cover() {
        let mut by_name = std::collections::HashMap::new();
        by_name.insert("9olq-6".to_string(), SystemInfo { id: 30000800, name: "9OLQ-6".into(),
            security: -0.5, constellation: String::new(), region: String::new(), faction: String::new() });
        let s = Systems::new(by_name, HashMap::new());
        // Hand-typed plain-text intel (single spaces, no showinfo tags). "Blue" is a standing
        // colour, but it begins the real name "Blue RandomAttac". The full span (incl. "Blue")
        // must be captured so the ESI cover can split it; previously "Blue" broke the run. (The
        // double-space *paste* form of this is split directly — see the paste tests.)
        let r = analyze("Blue RandomAttac Redhorn Mastro 9OLQ-6", &s, &noships(), &noknown(), 1, "ch", "Ariel Afuran");
        let (pilots, sysd, _) = resolve_report(&r, &["Blue RandomAttac", "Redhorn Mastro"], &s);
        assert_eq!(pilots, vec!["Blue RandomAttac".to_string(), "Redhorn Mastro".to_string()]);
        assert!(sysd.iter().any(|d| d == "9OLQ-6"), "systems={sysd:?}");
    }

    #[test]
    fn suffix_subphrase_pilot_is_dropped() {
        let s = systems();
        // "Chen Chen" is a contiguous suffix of "Dr Chen Chen" — the loose run (which
        // breaks on the 2-char "Dr") must not leak it as a second pilot.
        let r = analyze("Dr Chen Chen in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.pilots, vec!["Dr Chen Chen".to_string()]);
    }

    #[test]
    fn isk_amount_is_not_a_count() {
        let s = systems();
        // "334 million" is ISK; the count is the 2 hostiles.
        let r = analyze("ESS raid 2 Bellicose 334 million 6:00 Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.count, Some(2), "ISK amount must not inflate the count");
        // Spaced "kk" / "bill" magnitudes are ISK too, not a hostile count.
        let r2 = analyze("ESS raid 2 Bellicose 300 kk 6:00 Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.count, Some(2), "300 kk must not be counted: {:?}", r2.count);
        let r3 = analyze("ESS raid 2 Bellicose 5 bill 6:00 Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r3.count, Some(2), "5 bill must not be counted: {:?}", r3.count);
    }

    #[test]
    fn adjacent_names_not_leaked_as_subword() {
        let s = systems();
        // Two adjacent two-word names; "Helper" must not leak out as a standalone name.
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
        // The real format: one line, last region glued to trailing prose.
        let glued = "EVE System > Channel MOTD: TENERIFIS // IMMENSEA // IMPASS // CATCHPlease contact Corps";
        assert_eq!(parse_motd_regions(glued, &known), vec!["tenerifis", "immensea", "impass", "catch"]);
        // Unknown segments ("Cache" absent from `known`) are ignored.
        assert_eq!(parse_motd_regions("Channel MOTD:  Wicked Creek //  Cache", &known), vec!["wicked creek"]);
        // Non-ASCII prose before a region must not desync byte offsets (regression: the
        // boundary check used to index the original string with lowercased offsets).
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
        // No hint: ambiguous "C-J" stays unresolved.
        let r0 = analyze_ctx("hostiles in C-J", &sys, &noships(), &noknown(), 1, "ch", "x", None, &[], &std::collections::HashSet::new());
        assert!(r0.systems.is_empty(), "should stay ambiguous: {:?}", r0.systems);
        // Channel covers Tenerifis -> resolves to C-J6MT, not the Vale C-J7CR.
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
        // "0xtomorrow" starts with a digit, so the Title-case paths miss it.
        let r = analyze("0xtomorrow AGCP-I", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["0xtomorrow"], &s);
        assert_eq!(pilots, vec!["0xtomorrow".to_string()], "pilots={pilots:?}");
        // ISK/count tokens and system abbreviations may be candidates but ESI confirms no
        // character, so they never surface as pilots.
        let junk = analyze("334m 88A 1DH-SX in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(esi_resolve(&junk.pilots, &[]).is_empty(), "junk pilots: {:?}", junk.pilots);
        // Time tokens are not names ("4min" = 4 minutes for an ESS post).
        assert!(is_time_token("4min") && is_time_token("30s") && is_time_token("2h"));
        assert!(!is_time_token("0xtomorrow") && !is_time_token("c137m"));
    }

    #[test]
    fn trailing_apostrophe_stripped_from_name() {
        let s = systems();
        let r = analyze("MO-I1W PeshyHod'", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["PeshyHod"], &s);
        assert_eq!(pilots, vec!["PeshyHod".to_string()], "pilots={pilots:?}");
        // Internal apostrophes are preserved.
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
        // Capitalised mid-run it still can't anchor a name.
        let r2 = analyze("Currently camped", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(esi_resolve(&r2.pilots, &[]).is_empty(), "pilots: {:?}", r2.pilots);
    }

    #[test]
    fn anom_sig_keywords_alone_produce_nothing() {
        let s = systems();
        let a = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x");
        for kw in ["anom", "sig", "anomaly", "signature"] {
            let r = a(kw);
            assert!(r.anom_sigs.is_empty(), "{kw}: anom_sigs={:?}", r.anom_sigs);
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
            // None of "diamond"/"dia"/"rat"/"rats" survive as a pilot name.
            let pilots = esi_resolve(&r.pilots, &["Diamond", "Dia", "Rat", "Rats"]);
            assert!(
                !pilots.iter().any(|p| {
                    matches!(p.to_lowercase().as_str(), "diamond" | "dia" | "rat" | "rats")
                }),
                "{txt}: rats word as pilot: {pilots:?}"
            );
        }
        // Plain "rats" (no diamond) is still NPCs, never a pilot, and doesn't set the diamond flag.
        let plain = analyze("rats on gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!plain.diamond_rats, "plain rats set diamond flag");
        assert!(esi_resolve(&plain.pilots, &["Rats"]).is_empty(), "plain pilots: {:?}", plain.pilots);
    }

    #[test]
    fn anom_sig_code_badge_both_orders() {
        let s = systems();
        // Keyword BEFORE the code.
        let before = analyze("anom ABC-123", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(before.anom_sigs, vec![(AnomKind::Anomaly, "ABC-123".to_string())]);
        // Keyword AFTER the code.
        let after = analyze("ABC-123 sig", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(after.anom_sigs, vec![(AnomKind::Signature, "ABC-123".to_string())]);
        for r in [&before, &after] {
            assert!(esi_resolve(&r.pilots, &[]).is_empty(), "pilots: {:?}", r.pilots);
            assert!(r.systems.is_empty(), "systems: {:?}", r.systems);
        }
        // Badge strings.
        assert_eq!(alert_label(&before.anom_sigs[0]), "Anom ABC-123");
        assert_eq!(alert_label(&after.anom_sigs[0]), "Sig ABC-123");
    }

    /// Mirror of the card/alert badge formatting for the test above.
    fn alert_label((kind, code): &(AnomKind, String)) -> String {
        match kind {
            AnomKind::Anomaly => format!("Anom {code}"),
            AnomKind::Signature => format!("Sig {code}"),
        }
    }

    #[test]
    fn anom_code_that_is_a_real_system_stays_a_system() {
        // A real system whose name matches the anom-code shape ("ABC-123").
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
        assert!(r.anom_sigs.is_empty(), "real system made an anom badge: {:?}", r.anom_sigs);
        assert!(
            r.systems.iter().any(|d| d.name == "ABC-123"),
            "real system not detected: {:?}",
            r.systems
        );
    }

    #[test]
    fn chinese_hull_name_resolves_as_ship() {
        let s = systems();
        // Localised hull names (审判者级 = Retribution) are keys in the ship index.
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
        // class target + point/scram variants
        let r2 = analyze("2 marauders pointed", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.tackled && r2.tackled_targets.iter().any(|t| t == "Marauder"), "targets={:?}", r2.tackled_targets);
        let r3 = analyze("recon scrammed", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.tackled && r3.tackled_targets.iter().any(|t| t == "Recon"), "targets={:?}", r3.tackled_targets);
        // cap tackled stays distinct + escalated (and still fires the generic tackled).
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
        // "t3" / "t3s" = Tier-3 (Strategic) Cruiser; "etc" is a stop word, never a pilot.
        let r3 = analyze("3 t3s and a t3 roaming, etc", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r3.classes.iter().any(|c| c == "Strategic Cruiser"), "classes={:?}", r3.classes);
        assert!(!r3.pilots.iter().any(|p| p.eq_ignore_ascii_case("etc")), "pilots={:?}", r3.pilots);
        // Generic hull sizes are ship types, never pilots.
        let r4 = analyze("CRUISERS and battleships in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r4.classes.iter().any(|c| c == "Cruiser"), "classes={:?}", r4.classes);
        assert!(r4.classes.iter().any(|c| c == "Battleship"), "classes={:?}", r4.classes);
        assert!(esi_resolve(&r4.pilots, &[]).is_empty(), "pilots={:?}", r4.pilots);
        // An all-caps ship acronym ("DNI" = Drake Navy Issue) is the ship, not a pilot.
        let mut ships = noships();
        ships.insert("dni".into(), (37457, "Drake Navy Issue".into()));
        let r5 = analyze("DNI in Jita", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r5.ships.iter().any(|sh| sh.name == "Drake Navy Issue"), "ships={:?}", r5.ships);
        assert!(r5.pilots.is_empty(), "pilots={:?}", r5.pilots);
        assert!(r2.classes.iter().any(|c| c == "Logistics"), "classes={:?}", r2.classes);
        assert!(r2.classes.iter().any(|c| c == "Stealth Bomber"), "classes={:?}", r2.classes);
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
        // No real characters here: every blob resolves to nothing via ESI/cover.
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
        // Adjacent ship names with no separator were read as a 2-word pilot.
        let r = analyze("ZD1-Z2 Sabre Orthrus in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        for w in ["Sabre", "Orthrus", "Sabre Orthrus"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
        }
        assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "ships={:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Orthrus"), "ships={:?}", r.ships);
        // "and"-separated ship list.
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
        // "cythe" = "Scythe" missing the leading S; "fleet"+"issue" match the hull exactly, so
        // the window resolves to the hull instead of two pilot names.
        let r = analyze("cythe fleet issue tackled in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r.ships);
        // Neither the typo, the suffix words, nor the canonical hull leak into pilots.
        for w in ["cythe", "fleet issue", "fleet", "issue", "scythe", "cythe fleet issue"] {
            assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case(w)), "{w}: {:?}", r.pilots);
        }
        // The masked typo token must not also surface as a standalone Scythe.
        assert!(!r.ships.iter().any(|sh| sh.name == "Scythe"), "ships={:?}", r.ships);
        // Location + tackle keyword still parse around the hull.
        assert!(r.systems.iter().any(|d| d.name == "Rancer"), "systems={:?}", r.systems);
        assert!(r.tackled, "tackled keyword should fire");

        // The correctly spelled hull still matches via the exact path (fuzzy is last resort).
        let r2 = analyze("Scythe Fleet Issue in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r2.ships.iter().any(|sh| sh.name == "Scythe Fleet Issue"), "ships={:?}", r2.ships);

        // Single-word fuzzy documents the >= 5-char threshold: "draek" (5, a transposition of
        // "drake") matches; "drak" (4) is too short to fuzz and does NOT.
        let r3 = analyze("draek in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r3.ships.iter().any(|sh| sh.name == "Drake"), "ships={:?}", r3.ships);
        let r4 = analyze("drak in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(!r4.ships.iter().any(|sh| sh.name == "Drake"), "ships={:?}", r4.ships);
    }

    #[test]
    fn confirmed_pilot_near_a_hull_stays_a_pilot() {
        let s = systems();
        let ships = ships_with(&[("Cyclone Fleet Issue", 17634)]);
        // A confirmed character whose name is one edit from a multi-word hull ("Cyclon" vs
        // "Cyclone") must stay a pilot — the known-pilot guard suppresses the fuzzy ship match.
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
        sys.add_bridges(&[(1, 2)]); // Ansiblex O3-4MN <-> Rancer
        let r = analyze("O3-4MN Gate camp on Ansi", &sys, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.camp, "gate-camp keyword should fire");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Ansi")), "pilots={:?}", r.pilots);
        assert!(r.gates.iter().any(|g| g == "Rancer"), "the Ansi should lead to Rancer: {:?}", r.gates);
    }

    #[test]
    fn system_code_known_as_pilot_is_not_a_pilot() {
        let s = systems();
        // A real character is named "C-J"; in intel it still means the system.
        let known: std::collections::HashMap<String, i64> =
            [("c-j".to_string(), 2119528359i64)].into_iter().collect();
        let r = analyze("Gorika Galrog C-J in Rancer", &s, &noships(), &known, 1, "ch", "x");
        // ESI confirms "Gorika Galrog"; the flanked code "C-J" is left behind (a system), never
        // surfaced as a pilot.
        let (pilots, _, _) = resolve_report(&r, &["Gorika Galrog"], &s);
        assert_eq!(pilots, vec!["Gorika Galrog".to_string()], "pilots={pilots:?}");
    }

    #[test]
    fn plus_count_adds_to_named_pilots() {
        let s = systems();
        // "+20" means 20 more besides the named pilot(s): 1 + 20 = 21.
        let r = analyze("Gorika Galrog +20 in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Gorika Galrog"), "pilots={:?}", r.pilots);
        assert_eq!(r.count, Some(21), "pilots={:?}", r.pilots);
        // A lone named pilot with no number gets no count badge (the badge is for 3+).
        let r2 = analyze("Gorika Galrog in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.count, None, "pilots={:?}", r2.pilots);
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
    fn sisters_combat_scanner_is_probes_not_pilots() {
        let s = systems();
        let r = analyze("Sisters Combat Scanner in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.probes, Some(Probes::Combat), "probes={:?}", r.probes);
        // "Sisters Combat Scanner" is a probe item, not a character — ESI resolves it to nothing.
        assert!(esi_resolve(&r.pilots, &[]).is_empty(), "pilots={:?}", r.pilots);
    }

    #[test]
    fn drops_subphrase_pilots_works() {
        let mut p = vec!["Nine".to_string(), "Nine -3".to_string()];
        // Source has one "Nine" token, entirely inside "Nine -3": the standalone collapses.
        drop_subphrase_pilots(&mut p, &std::collections::HashSet::new(), "Nine -3");
        assert_eq!(p, vec!["Nine -3".to_string()]);
        // A char-linked name is protected even when a longer glued run contains it.
        let mut q = vec!["Callas Plaude".to_string(), "Callas Plaude Wolf".to_string()];
        let protect: std::collections::HashSet<String> = ["callas plaude".to_string()].into();
        drop_subphrase_pilots(&mut q, &protect, "Callas Plaude Wolf");
        assert!(q.contains(&"Callas Plaude".to_string()), "q={q:?}");
        // Occurrence-aware: two "Tiffanbrill" tokens (one standalone, one inside "Tiffanbrill
        // Dragon") keep BOTH the one-word and the two-word pilot.
        let mut t = vec!["Tiffanbrill".to_string(), "Tiffanbrill Dragon".to_string()];
        drop_subphrase_pilots(
            &mut t,
            &std::collections::HashSet::new(),
            "Tiffanbrill Tiffanbrill Dragon",
        );
        assert_eq!(t, vec!["Tiffanbrill".to_string(), "Tiffanbrill Dragon".to_string()], "t={t:?}");
        // But a single occurrence wholly inside the longer name still collapses (no spare token).
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
        // From real intel: "C-J" (system abbreviation) and "bubbled" were read as pilots.
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
        // The blob carries the trailing "cloaked … bubble" keywords, but ESI confirms only the
        // real pilot — "cloaked" / the glued form never survive.
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
        // Real players happen to be named "Navy" and "Comet"; neither should be read as
        // a pilot in "Federation Navy Comet".
        let known: std::collections::HashMap<String, i64> =
            [("navy".to_string(), 1i64), ("comet".to_string(), 2i64)].into_iter().collect();
        let r = analyze("Federation Navy Comet Docteur West in Rancer", &s, &ships, &known, 1, "ch", "x");
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Navy")), "pilots={:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Comet")), "pilots={:?}", r.pilots);
    }

    #[test]
    fn hedging_think_not_a_pilot_even_if_known() {
        let s = systems();
        // A real player is named "Think"; "i think they're in Rancer" must not read it as a pilot.
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
        // "Drifter" is an intel keyword, but here it ends the real name "High Plains Drifter".
        // A stop word must never be trimmed off a genuine name — only short connective words and
        // ENTIRELY-prose candidates are dropped.
        let r = analyze("High Plains Drifter in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["High Plains Drifter"], &s);
        assert_eq!(pilots, vec!["High Plains Drifter".to_string()], "pilots={pilots:?}");
    }

    #[test]
    fn other_side_and_theft_are_not_pilots() {
        let s = systems();
        // "other side" (positional filler) and "theft" (a structure-grief verb) leaked as
        // single-word pilots; neither is a player.
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
        // A linking word inside a name ("is", "of") must not split it into two pilots.
        for (m, want) in [
            ("Cult is Dead in Rancer", "Cult is Dead"),
            ("Lord of War in Rancer", "Lord of War"),
        ] {
            let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
            assert!(r.pilots.iter().any(|p| p == want), "{m} -> {:?}", r.pilots);
            assert!(!r.pilots.iter().any(|p| p == "Cult" || p == "Dead" || p == "War"), "{m} -> {:?}", r.pilots);
        }
        // A linking word at a name boundary is grammar, not part of the name.
        let r = analyze("Sevra is in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p == "Sevra is"), "pilots={:?}", r.pilots);
        // "X is Y" prose with a keyword isn't a name.
        let r2 = analyze("gate is camped in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.is_empty(), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn uppercase_x_multiplier_is_a_count() {
        let s = systems();
        // "X5"/"x5" both decorate a hostile count (case-insensitive). "X5" can also be part
        // of a pilot name, so we only assert the count is read here, not pilot exclusion.
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
        // "skhook" (a common fat-finger of "skyhook") still raises the Skyhook structure and
        // is not read as a pilot.
        let r = analyze("skhook theft in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.skyhook, "skyhook flag not set: {:?}", r.text);
        assert!(r.structures.iter().any(|(n, _)| n.as_str() == "Skyhook"), "structs={:?}", r.structures);
        assert!(r.pilots.is_empty(), "pilots={:?}", r.pilots);
        // A real (non-skyhook) word is not swept in by the typo tolerance.
        let r2 = analyze("Schook in Jita", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r2.skyhook, "schook wrongly flagged as skyhook");
    }

    #[test]
    fn descriptor_and_verb_words_are_not_pilots() {
        let s = systems();
        // From real logs: "Navy"/"Issue" (ship descriptors) and "jumped" (a verb) leaked.
        // The candidate is now a blob; ESI/cover claims the real name and drops the rest.
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
        // Celestials: planet/moon + number or roman, and the sun. The trailing number is a
        // location, never a hostile count.
        let p1 = analyze("planet 1 Jita", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(p1.celestials, vec!["Planet 1".to_string()]);
        assert!(p1.count.is_none(), "count={:?}", p1.count);
        let m = analyze("moon IV in Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(m.celestials, vec!["Moon IV".to_string()]);
        // Planet-moon form: the planet number matters, so "moon 5-3" is kept whole.
        let m53 = analyze("moon 5-3 Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(m53.celestials, vec!["Moon 5-3".to_string()]);
        assert!(m53.count.is_none(), "count={:?}", m53.count);
        // A pasted moon location folds the preceding roman planet in -> "Moon 6-12".
        let paste = analyze("Rancer VI - Moon 12", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(paste.celestials, vec!["Moon 6-12".to_string()], "cels={:?}", paste.celestials);
        // A lone "I" after "moon" is the pronoun, not roman 1 — no phantom "Moon I".
        let mi = analyze("moon I think it's clear Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert!(mi.celestials.is_empty(), "phantom celestial: {:?}", mi.celestials);
        let sun = analyze("camped at the sun Rancer", &systems(), &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(sun.celestials, vec!["Sun".to_string()]);
        assert_eq!(detect_structures("POS bash Rancer"), vec![("POS".to_string(), None)]);
        assert!(is_structure_word("pos"));
        assert!(detect_structures("hostiles in Rancer").is_empty());
        // structure abbreviations aren't pilots
        assert!(is_structure_word("fort") && is_structure_word("keep") && is_structure_word("astra"));
        // A two-word structure tail ("Beacon" in "Cyno Beacon") is masked, so it is never
        // also read as a pilot — only the structure is reported.
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
        assert_eq!(detect_probes("Probe tackled"), None); // the frigate
        assert_eq!(detect_probes("hostiles in Rancer"), None);

        // No double detection: the Probe frigate is dropped and it isn't a pilot.
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
        // A lone "Probe" is still the frigate.
        let r2 = analyze("Probe tackled", &s, &si, &noknown(), 1, "ch", "x");
        assert!(r2.ships.iter().any(|sh| sh.name.eq_ignore_ascii_case("probe")));
        // "prob" is shorthand for "probably", not scanning probes.
        assert!(analyze("prob cyno in Rancer", &s, &noships(), &noknown(), 1, "ch", "x").probes.is_none());
        assert_eq!(analyze("combat probes on dscan", &s, &noships(), &noknown(), 1, "ch", "x").probes, Some(Probes::Combat));
        // The PILOT "RSS Scanner Probe" (case-sensitive) must be a pilot, not a probe alert.
        let rp = analyze("RSS Scanner Probe tackled in Rancer", &s, &si, &noknown(), 1, "ch", "x");
        assert_eq!(rp.probes, None, "pilot name triggered a probe badge: {:?}", rp.probes);
        assert!(
            rp.pilots.iter().any(|p| p == "RSS Scanner Probe"),
            "RSS Scanner Probe not a pilot: {:?}",
            rp.pilots
        );
        // A genuine probe call in the same message still fires (different, real wording).
        assert_eq!(
            analyze("RSS Scanner Probe and Sisters Combat Scanner Probe on dscan", &s, &si, &noknown(), 1, "ch", "x").probes,
            Some(Probes::Combat),
            "real probes after the pilot name should still fire"
        );
    }

    #[test]
    fn parses_isk_amounts() {
        assert_eq!(parse_isk("ess 300kk 5 min", true), Some(300_000_000));
        assert_eq!(parse_isk("worth 1.5b", false), Some(1_500_000_000));
        assert_eq!(parse_isk("300 mil tag", false), Some(300_000_000));
        // Bare "m" reads as millions only with ESS context (collides with "4M-" shorthands).
        assert_eq!(parse_isk("ess 750m", true), Some(750_000_000));
        assert_eq!(parse_isk("loot 750m", false), None);
        // "4M-HGW" is a null-sec system code, never ISK (even with ESS in the line).
        assert_eq!(parse_isk("ess hostiles in 4M-HGW", true), None);
        assert_eq!(parse_isk("5 min", false), None);
        assert_eq!(parse_isk("Rancer 3 Drake +2", false), None);
        // ESS amounts below 50M are a time, not ISK ("30m" = 30 minutes), so they're ignored;
        // a real bank >= 50M still parses, and the floor is ESS-only.
        assert_eq!(parse_isk("ess robbed 30m", true), None);
        assert_eq!(parse_isk("ess reserve 30m bank", true), None);
        assert_eq!(parse_isk("ess 50m", true), Some(50_000_000));
        assert_eq!(parse_isk("ess 77m bank", true), Some(77_000_000));
        assert_eq!(parse_isk("30m loot", false), None); // non-ESS unaffected (bare m only ESS)
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
        // "5 min" -> "min" is time, not a name.
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
        // "the" is a stop word but a name connector -> "The Meek" keeps it.
        assert!(extract_pilots("384-IN The Meek").iter().any(|r| r == "The Meek"));
    }

    #[test]
    fn intel_descriptor_breaks_a_name_run() {
        // "cloaked" is a stop word AND an intel descriptor -> never part of a name.
        let out = extract_pilots("Cloaked Predator");
        assert!(!out.iter().any(|r| r.to_lowercase().contains("cloaked")), "out={:?}", out);
        // a run that is only stop words isn't a pilot either.
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
        // "Bob" appears on its own AND inside "Bob Smith" -> both are real, keep both.
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
        // All-lowercase, code-like middle word, no Title-Case anchor.
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
        // Plain-text log, a lone Title-Case name must still be offered for ESI.
        let r = analyze("Sevra in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p == "Sevra"), "pilots={:?}", r.pilots);
    }

    #[test]
    fn keywords_no_substring_false_trigger() {
        let s = systems();
        // "Bunk Helper" is a pilot run; "Helper" must not trigger HELP.
        let r = analyze("Bunk Boi Bunk Helper in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r.help, "Helper must not trigger help");
        // "Cynabal" the ship must not trigger CYNO.
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("cynabal".to_string(), (17720i64, "Cynabal".to_string()))].into_iter().collect();
        let r2 = analyze("Cynabal in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(!r2.cyno, "Cynabal must not trigger cyno");
        // Exact keywords still fire.
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
        // Real logs carry no url tags; a lowercase name next to a Title-Case one must
        // still become a candidate run (ESI/cover confirms it later).
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
        // Chinese killReport text (no url wrapper in the log) is recognised as a kill.
        let r = analyze("DZ Sharisa > 击杀：Wolf E Kristjansson", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.killmail, "should flag a kill from the Chinese keyword");
    }

    #[test]
    fn detects_single_ship_name() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
        // Case-insensitive: lower-case ship name is detected.
        let r = analyze("E-JCUS sabre", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(r.ships.iter().map(|sh| sh.name.clone()).collect::<Vec<_>>(), vec!["Sabre"]);
        // Single-word "Sabre" prefers the ship even if a pilot shares the name.
        // A compound pilot name ("Sabre Smith") prefers the pilot — no ship parsed.
        let r2 = analyze("Sabre Smith in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r2.ships.is_empty());
        assert_eq!(r2.systems.iter().map(|d| d.name.clone()).collect::<Vec<_>>(), vec!["Rancer"]);
    }

    #[test]
    fn extracts_pilot_candidates() {
        let s = systems();
        let r = analyze("Some Pilot tackled in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(esi_resolve(&r.pilots, &["Some Pilot"]), vec!["Some Pilot".to_string()]);
        // Common Title-Case intel phrases ("Gate Camp") reach ESI but resolve to no character.
        let r2 = analyze("Gate Camp in Rancer", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(esi_resolve(&r2.pilots, &[]).is_empty(), "pilots={:?}", r2.pilots);
    }

    #[test]
    fn amend_merges_ship_when_system_held_in_name_blob() {
        let s = systems();
        let sh: std::collections::HashMap<String, (i64, String)> =
            [("gila".to_string(), (17715i64, "Gila".to_string()))].into_iter().collect();
        // Two different scouts on the same hostile in C-J6MT. The system is held inside each
        // unresolved name blob ("C-J6MT Keeves" vs "Keeves C-J6MT"), and the second adds the ship.
        // They must still link by the shared pilot WORD ("keeves", not the held system) and merge
        // the Gila — otherwise the ship badge is stranded on a separate card.
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
        // Two reporters mention the SAME leading-digit character "1 Tap Machine". The leading "1"
        // is part of the NAME (a confirmed/known pilot), so it must NOT also be read as a hostile
        // count of 1, and the two mentions must merge — not split into two cards.
        let s = systems_with(&[("kzfv-4", "KZFV-4", 30100, -0.5)]);
        let ships = ships_with(&[("Exequror Navy Issue", 29344)]);
        // "1 Tap Machine" confirmed in the known cache (deterministic).
        let known: std::collections::HashMap<String, i64> =
            [("1 tap machine".to_string(), 1i64)].into_iter().collect();
        let a = analyze("1 Tap Machine ENI", &s, &ships, &known, 100, "ch", "Corn SilkTea");
        let b = analyze("KZFV-4* 1 Tap Machine", &s, &ships, &known, 130, "ch", "jhouzy");
        // Both mentions name the pilot "1 Tap Machine"; the leading "1" is the name, not a count.
        assert!(proposed(&a.pilots, "1 Tap Machine"), "A pilots={:?}", a.pilots);
        assert!(proposed(&b.pilots, "1 Tap Machine"), "B pilots={:?}", b.pilots);
        assert_eq!(a.count, None, "leading digit counted in A: {:?}", a.pilots);
        assert_eq!(b.count, None, "leading digit counted in B: {:?}", b.pilots);
        assert!(b.systems.iter().any(|d| d.name == "KZFV-4"), "B system={:?}", b.systems);
        // The re-mention amends the first (same pilot) — one merged card, not a split.
        let mut state = IntelState::default();
        state.push(a);
        assert!(state.try_amend(&b, 60, &s), "second mention should amend the first");
        assert_eq!(state.reports.len(), 1, "split into separate cards: {:?}", state.reports);
        assert!(state.reports[0].systems.iter().any(|d| d.name == "KZFV-4"), "system not merged");

        // Control: a genuine "3 Drake" count (not a confirmed pilot) STILL counts as 3 Drakes.
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
    #[test]
    fn kill_paste_extracts_victim_and_ship() {
        let s = systems();
        let ships = ships_with(&[("Loki", 29990)]);
        let known: std::collections::HashMap<String, i64> =
            [("lord road".to_string(), 1i64), ("road".to_string(), 2i64)].into_iter().collect();
        // English "Kill: <victim> (<ship>)".
        let r = analyze("Kill: Lord Road (Loki)", &s, &ships, &known, 1, "ch", "x");
        assert!(r.killmail, "killmail flag");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Lord Road")), "victim: {:?}", r.pilots);
        assert!(!r.pilots.iter().any(|p| p.eq_ignore_ascii_case("Road")), "bare Road: {:?}", r.pilots);
        assert!(r.ships.iter().any(|sh| sh.name == "Loki"), "ship: {:?}", r.ships);
        // Chinese, killword+colon glued to the name; hull absent from the (English) index.
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
        // "fibular" is a dictionary word, but here a real LINKED pilot in a paste -> kept.
        let r = analyze("fibular  detective spider  Q-K2T7", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("fibular")), "dropped: {:?}", r.pilots);
        // The same word as a lone non-paste mention is still dropped as prose.
        let known: std::collections::HashMap<String, i64> =
            [("fibular".to_string(), 5i64)].into_iter().collect();
        let r2 = analyze("fibular in Rancer", &s, &noships(), &known, 1, "ch", "x");
        assert!(!r2.pilots.iter().any(|p| p.eq_ignore_ascii_case("fibular")), "prose kept: {:?}", r2.pilots);
    }

    #[test]
    fn keyword_in_pasted_name_does_not_bail_paste() {
        let s = systems();
        // A double-space paste where one linked name contains a keyword ("fliet98 cyno") must not
        // bail the whole paste to the glued-run fallback - each segment surfaces as its own pilot,
        // never one >3-word blob (regression: a single "cyno" segment dropped every pilot).
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
        // Both "Lord Road" and a bare "Road" are real characters. A mention of "Lord Road" must
        // surface only the full name, never the tail "Road" as a second pilot.
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
        // The real report (kill notification paste): the tail "Road" must not re-surface.
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
        // Same reporter, no system (gate only), within grace -> amends.
        let follow = analyze("on 78- gate", &s, &noships(), &noknown(), 130, "ch", "Scout");
        assert!(state.try_amend(&follow, 60, &s));
        assert_eq!(state.reports.len(), 1);
        assert!(!state.reports[0].gates.is_empty());
        // A different system is a new sighting, not an amendment.
        let other = analyze("hostile in Jita", &s, &noships(), &noknown(), 140, "ch", "Scout");
        assert!(!state.try_amend(&other, 60, &s));
        // A clear is never amended into a sighting (it must not wipe ship info).
        let clear = analyze("Rancer clear", &s, &noships(), &noknown(), 150, "ch", "Scout");
        assert!(!state.try_amend(&clear, 60, &s));
    }

    #[test]
    fn known_pilots_match_with_subset_protection() {
        let s = systems();
        // A lower-case single-word known name is recognised.
        let k1: std::collections::HashMap<String, i64> =
            [("bigfoott".to_string(), 2i64)].into_iter().collect();
        let r = analyze("Rancer bigfoott", &s, &noships(), &k1, 1, "ch", "x");
        let (pilots, _, _) = resolve_report(&r, &["bigfoott"], &s);
        assert!(pilots.iter().any(|p| p.eq_ignore_ascii_case("bigfoott")), "{pilots:?}");
        // A name that is a subset of a longer one must not short-circuit it.
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
        // System code before the name is excluded; the ship is the parens content.
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
        // Scout A: a hyphenated system + a pilot with a digit in the name. Resolved as the live
        // reconcile would (C-J6MT freed as the location once "Pericle No1" is confirmed).
        let mut a = analyze("C-J6MT Pericle No1", &s, &noships(), &noknown(), 100, "ch", "Kobayashi Mika");
        apply_resolution(&mut a, &["Pericle No1"], &s);
        assert_eq!(a.pilots, vec!["Pericle No1".to_string()]);
        state.push(a);
        // Scout B (different reporter): same pilot, no system, adds the ship.
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
        assert!(!r.clear); // quoted -> not a status keyword
        assert_eq!(r.systems.len(), 1); // Rancer still a system
        // Mixed opening/closing quotes.
        let r2 = analyze("`Some Guy\" tackled", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(r2.pilots.iter().any(|p| p == "Some Guy"));
    }

    #[test]
    fn name_with_trailing_number_isnt_a_count() {
        let s = systems();
        // "Malcolm 41" / "Adama 80" are pilot names; the trailing number is part of the
        // name, not a hostile count.
        assert_eq!(analyze("8X6T-8 Malcolm 41", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
        assert_eq!(analyze("Adama 80 pls help", &s, &noships(), &noknown(), 1, "ch", "x").count, None);
        // A number after a *system* is still a count.
        assert_eq!(analyze("Rancer 5", &s, &noships(), &noknown(), 1, "ch", "x").count, Some(5));
    }

    #[test]
    fn loose_runs_keep_short_name_parts() {
        let s = systems();
        // Short components ("80", "R") stay in the run so the ESI cover can confirm the
        // real names ("Adama 80", "Lopatich R").
        let runs = loose_pilot_runs("Adama 80 Lopatich R", &noships(), &s);
        assert!(runs.iter().any(|r| r.contains("80")), "runs={:?}", runs);
        assert!(runs.iter().any(|r| r.split_whitespace().last() == Some("R")), "runs={:?}", runs);
        // A run of only short parts is dropped (needs a real >=3-letter word).
        assert!(loose_pilot_runs("80 90", &noships(), &s).is_empty());
    }

    #[test]
    fn system_detection_coverage() {
        let s = systems(); // Rancer, Jita, C-J6MT, Uitra, N3-JBX, Amarr, …
        let det = |m: &str| {
            let r = analyze(m, &s, &noships(), &noknown(), 1, "ch", "x");
            (r.systems.iter().map(|x| x.name.clone()).collect::<Vec<String>>(), r.gates.clone())
        };
        // A single system anywhere in the message is the location.
        assert_eq!(det("hostiles in Jita").0, vec!["Jita"]);
        assert_eq!(det("Jita").0, vec!["Jita"]);
        assert_eq!(det("5 reds Jita").0, vec!["Jita"]);
        // A null-sec code resolves to its system.
        assert_eq!(det("C-J6MT clear").0, vec!["C-J6MT"]);
        // EVE's route-waypoint "*" suffix is stripped.
        assert_eq!(det("Jita* hostiles").0, vec!["Jita"]);
        // Two systems: the first is the location, the second a gate.
        let (sysd, gates) = det("N3-JBX Uitra");
        assert_eq!(sysd, vec!["N3-JBX"]);
        assert!(gates.iter().any(|g| g == "Uitra"), "gates={gates:?}");
        // An abbreviated "<code> gate" resolves the gate's destination system.
        assert!(det("on C-J gate").1.iter().any(|g| g == "C-J6MT"), "{:?}", det("on C-J gate"));
        // A system mentioned alongside a pilot is still the location; the pilot is not a system.
        assert_eq!(det("Sevra in Jita").0, vec!["Jita"]);
        // A plain word that isn't a system never invents one.
        assert!(det("hostiles incoming").0.is_empty());
        // A lower-case word matching a system name IS the system ("clear in here" must NOT,
        // handled elsewhere) — a code is matched case-insensitively.
        assert_eq!(det("c-j6mt clear").0, vec!["C-J6MT"]);
    }

    #[test]
    fn detects_systems_count_and_flags() {
        let s = systems();

        let r = analyze("hostile in Rancer, 3 Drake +2", &s, &noships(), &noknown(), 100, "ch", "Scout");
        assert_eq!(r.systems.len(), 1);
        assert_eq!(r.systems[0].name, "Rancer");
        assert_eq!(r.count, Some(5)); // groups summed: 3 + 2
        assert!(!r.clear);

        assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x").clear);
        assert!(analyze("nv in Jita", &s, &noships(), &noknown(), 1, "ch", "x").no_visual);
        assert!(analyze("gate camp 1DQ1-A bubble up", &s, &noships(), &noknown(), 1, "ch", "x").camp);
        // One-word "gatecamp" fires the camp keyword and is never read as a pilot.
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
        // lower-case common words that are system names are not matched
        assert!(analyze("clear in here", &s, &noships(), &noknown(), 1, "ch", "x").systems.is_empty());
    }

    #[test]
    fn recognizes_battle_report_links_including_our_site() {
        // Our own site (eve-spai.com/br/<id>) is recognized like the external BR hosts.
        let ours = extract_links("gf all https://eve-spai.com/br/abc123def nice fight");
        assert!(
            ours.iter().any(|l| l.kind == LinkKind::BattleReport && l.url.contains("eve-spai.com/br/")),
            "{ours:?}"
        );
        // Existing BR hosts still recognized; a plain link is not a BR.
        assert!(extract_links("https://br.evetools.org/br/xyz")
            .iter()
            .any(|l| l.kind == LinkKind::BattleReport));
        assert!(!extract_links("https://eve-spai.com/about").iter().any(|l| l.kind == LinkKind::BattleReport));
    }

    // "I" is the one English word that is always capitalized, so it looks like a name. These
    // pin down that the pronoun never leaks as a pilot, while names that contain "I" still do.
    #[test]
    fn pronoun_i_never_a_pilot() {
        let s = systems();
        // The pronoun "I" must never survive resolution as a pilot (a 1-letter span is never a
        // character, so the cover skips it).
        let has_i = |names: &[String]| names.iter().any(|p| p.split_whitespace().any(|w| w == "I"));
        for txt in [
            "I think 5 reds in Jita",
            "I guess they left",
            "tackled one, I saw him warp Jita",
            "Rancer clear, I am going afk",
            "dunno where they went, I missed it",
            "I see a Sabre and I think a Loki",
            "i think reds incoming",          // lowercase pronoun
            "Bishopi I think he docked",       // pronoun right after a real name
            "warp to I and hold",              // pronoun mid-sentence by itself
        ] {
            let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "Spai");
            let resolved = esi_resolve(&r.pilots, &["Bishopi", "Sabre"]);
            assert!(!has_i(&resolved), "pronoun 'I' leaked as a pilot in {txt:?}: {resolved:?}");
        }
        // The pronoun must not glue an adjacent real name into a phantom: "Bishopi" resolves
        // on its own, but never as "Bishopi I think".
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
        // The real cache has clol23, rm712, wenmg as confirmed pilots; MuskQAQ is not yet cached.
        let known: std::collections::HashMap<String, i64> = [
            ("clol23".to_string(), 2124249172i64),
            ("rm712".to_string(), 2117556515),
            ("wenmg".to_string(), 2121075688),
        ]
        .into_iter()
        .collect();
        let plain = "clol23 MuskQAQ rm712 wenmg 9-OUGJ";
        let r = analyze(plain, &s, &noships(), &known, 1, "ch", "TreeBeard Elderling");
        // Kept whole at parse time — ESI decides. Once it rejects the 4-word whole and confirms the
        // handles (MuskQAQ included), the reconcile splits it into them.
        let split = esi_resolve(&r.pilots, &["clol23", "MuskQAQ", "rm712", "wenmg"]);
        let lc: Vec<String> = split.iter().map(|p| p.to_lowercase()).collect();
        for want in ["clol23", "rm712", "wenmg", "muskqaq"] {
            assert!(lc.contains(&want.to_string()), "missing {want}: {:?}", split);
        }

        // Guard: a 2-word name with ONE known handle is NEVER split (kept whole, ESI confirms it).
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

        // Reported case 1: system + single-word Title names, NO cache.
        let r = analyze("L-FM3P  Gliar  Mliarvis  Sliarhia", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar", "mliarvis", "sliarhia"]);
        assert!(r.systems.iter().any(|d| d.name == "L-FM3P"), "system kept: {:?}", r.systems);

        // Reported case 2: lowercase + digit handles, system last.
        let r = analyze("clol23  MuskQAQ  rm712  wenmg  9-OUGJ", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["clol23", "muskqaq", "rm712", "wenmg"]);

        // Reported case 3: Title+digit names, system carries the "*" route marker (stripped).
        let r = analyze("YPW-M4*  Boris95  BorisDread95  Destroyer95", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["boris95", "borisdread95", "destroyer95"]);
        assert!(r.systems.iter().any(|d| d.name == "YPW-M4"), "system: {:?}", r.systems);

        // Two-word names split AT the double space, never within it.
        let r = analyze("L-FM3P  First Last  Second Guy", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["first last", "second guy"]);

        // 3+ spaces also delimit.
        let r = analyze("L-FM3P    Gliar    Mliarvis", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar", "mliarvis"]);

        // Identical result with the cache populated.
        let known: std::collections::HashMap<String, i64> =
            [("gliar".to_string(), 1i64), ("mliarvis".to_string(), 2), ("sliarhia".to_string(), 3)]
                .into_iter()
                .collect();
        let r = analyze("L-FM3P  Gliar  Mliarvis  Sliarhia", &s, &noships(), &known, 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar", "mliarvis", "sliarhia"]);

        // A ship segment is a ship, not a pilot (still a valid anchor).
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("sabre".to_string(), (22456i64, "Sabre".to_string()))].into_iter().collect();
        let r = analyze("L-FM3P  Gliar  Sabre", &s, &ships, &noknown(), 1, "ch", "x");
        assert_eq!(lc(&r), vec!["gliar"], "sabre must not be a pilot");
        assert!(r.ships.iter().any(|sh| sh.name == "Sabre"), "sabre is a ship: {:?}", r.ships);
    }

    #[test]
    fn double_space_falls_back_on_prose_and_bad_grammar() {
        let s = systems(); // has Jita, Rancer, 78-AAA, …
        let pilots = |t: &str| {
            let mut v = analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").pilots;
            v.sort();
            v
        };
        // Prose with a stray double space and an embedded system: NOT a paste — and the normal
        // cap-tackle detection still works (regression for "rorqual  pointed in Jita").
        assert!(
            analyze("rorqual  pointed in Jita", &s, &noships(), &noknown(), 1, "ch", "x").cap_tackled,
            "cap detection must survive a stray double space"
        );
        // Prose words that aren't on the stop list ("lads") are candidates, but ESI confirms
        // no character — so the resolved set is empty.
        for t in [
            "reds  pointed in Jita",
            "they  warped off to Jita",
            "got him  tackled in Jita now",
            "Rancer  is clear now lads",
        ] {
            let resolved = esi_resolve(&pilots(t), &[]);
            assert!(resolved.is_empty(), "prose treated as paste for {t:?}: {resolved:?}");
        }
        // The decisive property: for any NON-paste input, the double-space hint must not change the
        // parse versus the same text with single spaces (it falls back to normal logic). This
        // covers casual chat / bad grammar with stray double spaces, including all-lowercase runs
        // that normal logic may already glue (e.g. "cats love fish") — the hint neither helps nor
        // harms there.
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
        // A paste whose tail is prose: the blob reaches ESI, which claims the real name and
        // drops the prose words ("they"/"warped" never survive as a pilot).
        let r = analyze("Rancer  Gliar  they all warped off already", &s, &noships(), &noknown(), 1, "ch", "x");
        let resolved = esi_resolve(&r.pilots, &["Gliar"]);
        assert_eq!(resolved, vec!["Gliar".to_string()], "prose tail leaked: {resolved:?}");
    }

    #[test]
    fn lowercase_clear_rain_pilot_detected() {
        let s = systems();
        // The real pilot is the all-lowercase "clear rain" (char 521632954). In a plain-text
        // log line it can't be found heuristically (name_part needs a capital, and "clear" is a
        // stop/clear word), and it isn't in the cache — so it relies on the allowlist.
        let r = analyze("Rancer clear rain nemesis on gate", &s, &noships(), &noknown(), 1, "ch", "Spai");
        assert!(r.pilots.iter().any(|p| p == "clear rain"), "clear rain not a pilot: {:?}", r.pilots);
        assert!(!r.clear, "pilot name 'clear rain' spoofed a clear status");
        // A genuine clear (no "rain") still reads as clear.
        assert!(analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "Spai").clear);
    }

    #[test]
    fn clear_loses_to_threats() {
        let s = systems();
        let ships: std::collections::HashMap<String, (i64, String)> =
            [("nemesis".to_string(), (11377i64, "Nemesis".to_string()))].into_iter().collect();
        // Plain-text (log) form: "clear" leaks from a name, but a dropper/bubble/ship is present.
        let r = analyze("Rancer hot dropper bubble clear rain nemesis", &s, &ships, &noknown(), 1, "ch", "Spai");
        assert!(!r.clear, "clear should lose to threats");
        // A genuine clear with no threat still reads as clear.
        let r2 = analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "Spai");
        assert!(r2.clear, "pure clear lost");
        // A Title-case name starting with "Clear" is one pilot, and doesn't read as clear.
        let r3 = analyze("got Clear Rain on gate", &s, &noships(), &noknown(), 1, "ch", "Spai");
        let (pilots, _, _) = resolve_report(&r3, &["Clear Rain"], &s);
        assert!(pilots.iter().any(|p| p == "Clear Rain"), "name split: {pilots:?}");
        assert!(!r3.clear, "name 'Clear Rain' spoofed clear");
    }

    #[test]
    fn pilot_name_with_keyword_words_and_trailing_ship_note() {
        // "Rage Starscythe > DUO-51  Roadman HighSec CynoLighter likely prospect" (url-stripped):
        // the 3-word linked pilot name embeds keyword-ish words ("HighSec", "CynoLighter") and is
        // followed by the reporter's typed note "likely prospect" (a stop word + the pilot's ship).
        // The name must surface as its own 3-word candidate, not be glued into an unresolvable
        // >3-word blob, and "Prospect" is read as a ship. Both the pasted (double-space) and the
        // single-space form. Case is not consulted.
        let s = systems();
        let ships = ships_with(&[("Prospect", 33468)]);
        // The pasted (double-space) form, the single-space form (a leading system code is trimmed
        // off the over-length run), and the same tail on a bare name.
        for t in [
            "DUO-51  Roadman HighSec CynoLighter likely prospect",
            "DUO-51 Roadman HighSec CynoLighter likely prospect",
            "Roadman HighSec CynoLighter likely prospect",
        ] {
            let r = analyze(t, &s, &ships, &noknown(), 1, "ch", "Rage Starscythe");
            assert!(
                r.pilots.iter().any(|p| p == "Roadman HighSec CynoLighter"),
                "{t}: {:?}",
                r.pilots
            );
            assert!(
                !r.pilots.iter().any(|p| {
                    p.to_lowercase().contains("likely") || p.to_lowercase().contains("prospect")
                }),
                "{t} leaked prose/ship into a pilot: {:?}",
                r.pilots
            );
            assert!(r.ships.iter().any(|sh| sh.name == "Prospect"), "{t}: {:?}", r.ships);
        }
    }

    #[test]
    fn pluralised_multiword_and_ies_hulls() {
        let s = systems();
        let ships = ships_with(&[("Osprey Navy Issue", 29990), ("Osprey", 620), ("Harpy", 11381)]);
        // Both the naive "-s" and the correct "-ies" plural of a multi-word hull resolve.
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
        // Single-word "-ies": harpies -> Harpy.
        let r = analyze("harpies on grid", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Harpy"), "{:?}", r.ships);
        // "-es" on a hull that already ends in s ("Ares" -> "Areses"), and a "-s" plural of an
        // "-e"-ending hull that must NOT be mangled by the "-es" try ("Bellicose" -> "Bellicoses").
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
        // "-L" / "-3" are alt-name suffixes, not system shorthands, and must stay on the name.
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
        // Even if the name is (wrongly) in the known-pilot cache, an exact hull name
        // is the ship, not a pilot.
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
        // Plain text (tags stripped): "Amarr slave 3424" is a pilot, not the Amarr
        // system; SV5-8N stays the system.
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
        // A specific code is recognised too.
        let r2 = analyze("Jita N968 sig", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.wh_type.as_deref(), Some("N968"));
        // Plain intel isn't a wormhole.
        let r3 = analyze("Rancer clear", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(!r3.wormhole);
        assert!(r3.wh_type.is_none());
    }

    #[test]
    fn neighbour_second_system_becomes_gate() {
        // Rancer (1) and Jita (2) as gate neighbours.
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
        // R959-U (1) and Agaullores (2) are NOT gate neighbours (a wormhole link).
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
        // R959-U's gate neighbour is some other system (99), not Agaullores — so we *know*
        // Agaullores isn't reachable by gate from it.
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
        // Two C-J systems → "c-j" is an ambiguous prefix (as live), so it isn't auto-resolved
        // as a system; a character is also named "c-j". In "c-j gate" it's the gate, not a player.
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
        // "no bubble" must not set bubble; "YPW gate" resolves to the full system name.
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
        // "status" is a request keyword: not a pilot, and stays informational.
        let q = analyze("status in Rancer?", &s, &noships(), &noknown(), 1, "ch", "x");
        assert!(q.status);
        assert!(!q.pilots.iter().any(|p| p.eq_ignore_ascii_case("status")));
    }

    #[test]
    fn lowercase_english_word_dropped_but_names_and_multiword_kept() {
        let s = systems();
        // A character is named "Carpet" (a plain English word, not a stop-word or ship). A LOWERCASE
        // mention is prose → dropped before ESI; a Capitalised mention is a name → kept; a multi-word
        // lowercase run still goes to ESI (the dictionary drop is single-word only).
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
        // C-J6MT neighbours 5E-CFL and SV5-8N.
        let adj = HashMap::from([(5i64, vec![10i64, 9]), (10, vec![5]), (9, vec![5])]);
        let s = Systems::new(by_name, adj);
        // A short prefix matching a neighbour name resolves to the full system.
        let r = analyze("C-J6MT 5e gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("5E-CFL"));
        // A 2-char letter prefix matches the other neighbour too.
        let r2 = analyze("C-J6MT sv gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("SV5-8N"));
    }

    #[test]
    fn gate_disambiguates_abbrev_via_context() {
        use std::collections::HashMap;
        let by_name = [
            ("d-pnsn", "D-PNSN", 1i64, -0.4),
            ("c-j6mt", "C-J6MT", 2, -0.6),
            ("c-jeez", "C-JEEZ", 3, -0.5), // makes "C-J" globally ambiguous
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
        let adj = HashMap::from([(1i64, vec![2i64]), (2, vec![1])]); // D-PNSN <-> C-J6MT
        let s = Systems::new(by_name, adj);
        // A system in the message gives the primary; "C-J" resolves to the neighbour.
        let r = analyze("D-PNSN C-J gate", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
        // No system in the message: the channel's last system (context) disambiguates.
        let r2 = analyze_ctx("C-J gate", &s, &noships(), &noknown(), 1, "ch", "x", Some(1), &[], &std::collections::HashSet::new());
        assert_eq!(r2.gates.first().map(|s| s.as_str()), Some("C-J6MT"));
    }

    #[test]
    fn detects_cap_tackled_variations() {
        let s = systems();
        let cap = |t: &str| analyze(t, &s, &noships(), &noknown(), 1, "ch", "x").cap_tackled;
        assert!(cap("Rancer cap tackled"));
        assert!(cap("rorqual  pointed in Jita")); // double space, words apart
        assert!(cap("dread scrammed on gate"));
        assert!(cap("carrier takled")); // typo
        assert!(cap("super got scram"));
        // Both a cap word and a tackle word are required.
        assert!(!cap("cap stable"));
        assert!(!cap("tackled a frigate"));
    }

    #[test]
    fn ess_time_ignores_isk_amount() {
        let s = systems();
        // "5 min" is the timer; "77m bank" is ISK, not 77 minutes.
        let r = analyze("TPG-DD ESS 5 min 77m bank", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r.ess_time.as_deref(), Some("5m"));
        // Reserve allows up to 45 min.
        let r2 = analyze("ESS reserve 30 min", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r2.ess_time.as_deref(), Some("30m"));
        // Normal bank rejects an over-max minute value.
        let r3 = analyze("ESS robbed 30m", &s, &noships(), &noknown(), 1, "ch", "x");
        assert_eq!(r3.ess_time, None);
        // A digit glued to a full-word unit ("30seconds") is the timer, not a pilot.
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
        // Abbreviated null-sec codes resolve by unique prefix; the gate is captured
        // and not double-listed as a plain system.
        let r = analyze("C-J +20 on 78- gate", &s, &noships(), &noknown(), 1, "ch", "Scout");
        assert_eq!(r.count, Some(20));
        assert_eq!(r.gates.first().map(|s| s.as_str()), Some("78-AAA"));
        assert_eq!(
            r.systems.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["C-J6MT"],
        );

        // A bare number used as a gate must not also be a hostile count.
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

    /// `systems()` plus one extra null-sec code, for repro lines that name an unlisted system.
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
        // "mio" (and a trailing-dot "mio.") is a common "million" abbreviation → 1e6, ungated by
        // ESS context (unlike bare "m", which stays ESS-only).
        assert_eq!(parse_isk("ess 346mio", true), Some(346_000_000));
        assert_eq!(parse_isk("346mio", false), Some(346_000_000));
        assert_eq!(parse_isk("worth 12 mio", false), Some(12_000_000));
        assert_eq!(parse_isk("ess 50mio.", true), Some(50_000_000));
        // Bare "m" is unchanged: ESS-only, so a null-sec shorthand isn't read as ISK.
        assert_eq!(parse_isk("loot 750m", false), None);
        assert_eq!(parse_isk("ess hostiles in 4M-HGW", true), None);
    }

    #[test]
    fn htg0_is_a_pilot_mskr1_stays_a_system() {
        // Repro line A: "MSKR-1 Htg-0 +5 gnosis 3x, Slasher, ESS 346mio" (reporter Duke Dekker).
        // MSKR-1 (all-caps code) is the system; Htg-0 (Title-case + digit) is a pilot, not a code;
        // both hulls are detected despite the "3x" count and the commas; ESS fires; 346mio = 346M.
        let s = systems_with(&[("mskr-1", "MSKR-1", 99, -0.5)]);
        let ships = ships_with(&[("Gnosis", 3756), ("Slasher", 585)]);
        let r = analyze(
            "MSKR-1 Htg-0 +5 gnosis 3x, Slasher, ESS 346mio",
            &s, &ships, &noknown(), 1, "ch", "Duke Dekker",
        );
        // Both hulls (the comma after "3x"/"Slasher" and the "3x" count don't drop Gnosis).
        assert!(r.ships.iter().any(|sh| sh.name == "Gnosis"), "ships={:?}", r.ships);
        assert!(r.ships.iter().any(|sh| sh.name == "Slasher"), "ships={:?}", r.ships);
        // ESS flag + the ISK amount via the "mio" suffix.
        assert!(r.ess, "ESS flag should fire: {:?}", r.text);
        assert_eq!(r.isk, Some(346_000_000), "isk={:?}", r.isk);
        // "346mio" is an amount, never a pilot candidate.
        assert!(!has_pilot_token(&r.pilots, "346mio"), "amount leaked as pilot: {:?}", r.pilots);
        // Htg-0 is proposed as a pilot (held in the location blob); MSKR-1 frees as the system once
        // ESI confirms the name (mirrors the live reconcile, like the other held-model tests).
        assert!(proposed(&r.pilots, "Htg-0"), "Htg-0 not proposed: {:?}", r.pilots);
        let (pilots, sysd, gates) = resolve_report(&r, &["Htg-0"], &s);
        assert_eq!(pilots, vec!["Htg-0".to_string()], "resolved pilots={pilots:?}");
        assert_eq!(sysd, vec!["MSKR-1".to_string()], "resolved systems={sysd:?}");
        assert!(gates.is_empty(), "gates={gates:?}");
    }

    #[test]
    fn code_pattern_name_without_real_system_is_a_pilot() {
        // `looks_like_system_code` is a CASE-INSENSITIVE pattern hint: every casing of a
        // code shape matches the pattern (authoritative system-ness is a real lookup).
        for t in ["Htg-0", "htg-0", "HTG-0", "MSKR-1", "Zzz-9", "zzz-9"] {
            assert!(looks_like_system_code(t), "{t} should match the code pattern");
        }
        assert!(!looks_like_system_code("Jean-Luc"), "a long-segment name is not a code");
        let s = systems_with(&[("mskr-1", "MSKR-1", 99, -0.5)]);
        // Known cache (case-insensitive) carries BOTH a code-shaped name with no real system
        // and a real system code. Only the former is a player.
        let known: std::collections::HashMap<String, i64> =
            [("zzz-9".to_string(), 42i64), ("mskr-1".to_string(), 7i64)].into_iter().collect();
        let r = analyze("Zzz-9 MSKR-1 tackled", &s, &noships(), &known, 1, "ch", "x");
        // No real "Zzz-9" system → fair game for a pilot, even from the known cache.
        assert!(proposed(&r.pilots, "Zzz-9"), "code-shaped name not a pilot: {:?}", r.pilots);
        // A real system code is the system, never a player who shares its name.
        assert!(!has_pilot_token(&r.pilots, "MSKR-1"), "system code leaked as pilot: {:?}", r.pilots);
        assert!(r.systems.iter().any(|d| d.name == "MSKR-1"), "MSKR-1 not the system: {:?}", r.systems);
    }

    #[test]
    fn pilot_word_that_is_a_hull_is_not_also_a_ship() {
        // Repro line B: "bovine worm" is a (confirmed) PILOT whose second word is the Worm hull —
        // it must be the pilot only, not also a ship.
        let s = systems();
        let ships = ships_with(&[("Worm", 17619)]);
        let known: std::collections::HashMap<String, i64> =
            [("bovine worm".to_string(), 1i64)].into_iter().collect();
        let r = analyze("bovine worm", &s, &ships, &known, 1, "ch", "x");
        assert!(r.pilots.iter().any(|p| p.eq_ignore_ascii_case("bovine worm")), "pilots={:?}", r.pilots);
        assert!(!r.ships.iter().any(|sh| sh.name == "Worm"), "Worm inside pilot span leaked: {:?}", r.ships);
        // Control: a hull that merely shares a word with a SEPARATE name (single-word pilot "Bob")
        // is still a ship — only a hull INSIDE a multi-word pilot span is suppressed.
        let ctrl = analyze("Bob in a Worm", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(ctrl.ships.iter().any(|sh| sh.name == "Worm"), "control Worm missing: {:?}", ctrl.ships);
    }

    #[test]
    fn noise_punctuation_between_tokens_does_not_break_detection() {
        // Commas/asterisks separating tokens are stripped by the tokenizer, but the apostrophe and
        // hyphen (real name characters) are preserved within a token.
        assert_eq!(tokenize("Slasher, ESS* 346mio"), vec!["Slasher", "ESS", "346mio"]);
        assert_eq!(tokenize("O'Brien I-Pustelga Htg-0"), vec!["O'Brien", "I-Pustelga", "Htg-0"]);
        // End-to-end: a comma + asterisk between a hull and "ESS" doesn't hide either.
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

    /// Volltz's WYF8-8 line: five pilots followed by the system token. The single-space list
    /// (a known-cache kill-list) must not collapse its five confirmed names into one unresolvable
    /// mega-blob, and the double-space paste must surface each — both plus the WYF8-8 system.
    #[test]
    fn wyf8_kill_list_keeps_all_five_pilots() {
        let s = systems_with(&[("wyf8-8", "WYF8-8", 30002126, -0.4)]);
        let reals =
            ["BoneChilling Chelien", "Gonzilla", "Krombopulous Jaynara", "Rollboy", "ShadowClown-Z"];
        let known: std::collections::HashMap<String, i64> =
            reals.iter().enumerate().map(|(i, r)| (r.to_lowercase(), i as i64 + 1)).collect();
        // Both the in-game double-space paste and a single-space typed list.
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

    /// Amending a WYF8-8 sighting with a second message about the same system unions the pilot
    /// sets — no name from either message is dropped.
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

    /// Reverse amend end-to-end via `analyze`: a reporter posts content with NO system
    /// ("Rifter Punisher +5"), which is stashed, then posts just the system ("FN0-QS") within
    /// the grace window. The system report absorbs the orphan's ships and count, and the orphan
    /// buffer is emptied.
    #[test]
    fn reverse_amend_revives_systemless_content() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let ships = ships_with(&[("Rifter", 587), ("Punisher", 597)]);
        let mut state = IntelState::default();

        // First message: content, no system — discarded today, so we stash it (as the watcher will).
        let orphan = analyze("Rifter Punisher +5", &s, &ships, &noknown(), 100, "intel", "Scout");
        assert!(orphan.systems.is_empty(), "orphan should have no system: {:?}", orphan.systems);
        assert!(!orphan.ships.is_empty(), "orphan should carry ships: {:?}", orphan.ships);
        assert_eq!(orphan.count, Some(5), "orphan count: {:?}", orphan.count);
        state.stash_orphan(orphan, 60, 100);

        // Second message: the system, 5s later. It reverse-amends the orphan.
        let mut sysmsg = analyze("FN0-QS", &s, &ships, &noknown(), 105, "intel", "Scout");
        assert_eq!(state.reverse_amend(&mut sysmsg, 60), 1, "one orphan should merge");
        assert!(sysmsg.systems.iter().any(|d| d.name == "FN0-QS"), "system lost: {:?}", sysmsg.systems);
        for hull in ["Rifter", "Punisher"] {
            assert!(sysmsg.ships.iter().any(|sh| sh.name == hull), "ship {hull} missing: {:?}", sysmsg.ships);
        }
        assert_eq!(sysmsg.count, Some(5), "count not carried: {:?}", sysmsg.count);
        assert!(state.orphans.is_empty(), "orphan buffer not emptied: {:?}", state.orphans.len());
    }

    /// The status/threat flags of a revived orphan are OR-ed into the system report.
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

    /// An orphan older than the grace window is NOT merged — and is dropped.
    #[test]
    fn reverse_amend_ignores_stale_orphan() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let ships = ships_with(&[("Rifter", 587)]);
        let mut state = IntelState::default();
        let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
        state.stash_orphan(orphan, 60, 100);
        // 61s later — just outside the window.
        let mut sysmsg = analyze("FN0-QS", &s, &ships, &noknown(), 161, "intel", "Scout");
        assert_eq!(state.reverse_amend(&mut sysmsg, 60), 0, "stale orphan should not merge");
        assert!(sysmsg.ships.is_empty(), "stale ship leaked: {:?}", sysmsg.ships);
        assert!(state.orphans.is_empty(), "stale orphan should be dropped: {:?}", state.orphans.len());
    }

    /// A different reporter or a different channel does NOT merge; a fresh non-matching orphan is
    /// kept for a later system of its own.
    #[test]
    fn reverse_amend_only_same_reporter_and_channel() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let ships = ships_with(&[("Rifter", 587)]);
        let mut state = IntelState::default();
        let orphan = analyze("Rifter +3", &s, &ships, &noknown(), 100, "intel", "Scout");
        state.stash_orphan(orphan, 60, 100);

        // Different reporter, same channel, within grace: no merge, orphan kept.
        let mut other_rep = analyze("FN0-QS", &s, &ships, &noknown(), 110, "intel", "SomeoneElse");
        assert_eq!(state.reverse_amend(&mut other_rep, 60), 0, "different reporter merged");
        assert!(other_rep.ships.is_empty(), "leaked into other reporter: {:?}", other_rep.ships);
        assert_eq!(state.orphans.len(), 1, "fresh non-matching orphan should be kept");

        // Same reporter, different channel: no merge, orphan still kept.
        let mut other_chan = analyze("FN0-QS", &s, &ships, &noknown(), 115, "other", "Scout");
        assert_eq!(state.reverse_amend(&mut other_chan, 60), 0, "different channel merged");
        assert_eq!(state.orphans.len(), 1, "orphan should still be kept");

        // Same reporter, same channel: now it merges.
        let mut mine = analyze("FN0-QS", &s, &ships, &noknown(), 120, "intel", "Scout");
        assert_eq!(state.reverse_amend(&mut mine, 60), 1, "same reporter/channel should merge");
        assert!(mine.ships.iter().any(|sh| sh.name == "Rifter"), "ship missing: {:?}", mine.ships);
        assert!(state.orphans.is_empty(), "orphan should be consumed");
    }

    /// A `clear` system report does NOT reverse-amend (a clear must stand alone).
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
        // The still-fresh orphan is retained for a later non-clear system message.
        assert_eq!(state.orphans.len(), 1, "orphan should be kept after a clear: {:?}", state.orphans.len());
    }

    /// `stash_orphan` prunes orphans older than the grace window, keeping the buffer bounded.
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
        // At t=200 the first (age 100) is pruned, the second (age 80) too, then the new one added.
        state.stash_orphan(mk(200), 60, 200);
        assert_eq!(state.orphans.len(), 1, "only the fresh orphan should remain");
        assert_eq!(state.orphans[0].received, 200);
    }

    /// A wormhole connection to Thera named AFTER a real system ("<Sys> Thera hole", "wh to
    /// Thera") must keep the real system as the primary location — Thera is a wh destination
    /// reference, not where the activity is — and must not become a bogus gate. Thera is still
    /// the primary when it genuinely IS the sole location.
    #[test]
    fn thera_wormhole_ref_does_not_override_primary() {
        // Rancer (1) adjacent to Jita (2); Thera not adjacent to anything.
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
        // The real system stays primary; Thera is neither primary nor a gate.
        for (line, primary) in [
            ("Rancer Thera hole", "Rancer"),
            ("Jita Thera hole", "Jita"),
            ("Rancer wh to Thera", "Rancer"),
            ("3 reds Rancer Thera hole", "Rancer"),
            ("C-J Thera hole", "C-J6MT"),
            // "Thera hole" first, real system after: the real system is still the location.
            ("Thera hole Rancer", "Rancer"),
        ] {
            let r = analyze(line, &s, &noships(), &noknown(), 1, "ch", "x");
            let (_p, sysd, gates) = resolve_report(&r, &[], &s);
            assert_eq!(sysd, vec![primary.to_string()], "{line:?}: primary system: {sysd:?}");
            assert!(!gates.iter().any(|g| g.eq_ignore_ascii_case("Thera")), "{line:?}: Thera became a gate: {gates:?}");
            assert!(matches!(r.wh_dest, Some(crate::wormholes::DestClass::Thera)), "{line:?}: wh_dest: {:?}", r.wh_dest);
        }
        // Thera as the genuine, sole location is still detected as the primary system.
        let r = analyze("hostiles in Thera camped", &s, &noships(), &noknown(), 1, "ch", "x");
        let (_p, sysd, _g) = resolve_report(&r, &[], &s);
        assert_eq!(sysd, vec!["Thera".to_string()], "genuine Thera location: {sysd:?}");
    }

    /// A system/gate code that also matches an inactive character ("UALX", both a system and a
    /// stale cached name) resolves to the SYSTEM, never a pilot — whether the character is active
    /// or demoted for inactivity.
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

    /// st0rkant's X5-0EM line: a pasted system + pasted pilot + a hand-typed "+count ships" tail.
    /// The tail must NOT be glued into a pilot blob (which would drop the pilot AND mask the hulls)
    /// — the pilot, the system, and every hull (incl. the "kikis" -> Kikimora nickname) resolve.
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
        // Every hull is detected (the ship words are not masked inside a pilot blob).
        for hull in ["Kikimora", "Flycatcher", "Kirin"] {
            assert!(r.ships.iter().any(|sh| sh.name == hull), "hull {hull} missing: {:?}", r.ships);
        }
        // The named pilot plus the "+12" more (named + N convention).
        assert_eq!(r.count, Some(13), "count: {:?}", r.count);
        // The pilot resolves and the system is X5-0EM.
        let (pilots, sysd, _g) = resolve_report(&r, &["dix otto"], &s);
        assert_eq!(pilots, vec!["dix otto".to_string()], "pilots: {pilots:?}");
        assert_eq!(sysd, vec!["X5-0EM".to_string()], "system: {sysd:?}");
    }

    #[test]
    fn unresolved_caps_code_in_gate_not_a_pilot() {
        // DT and UALX are abbreviated system names we can't resolve — neither is a pilot.
        let s = systems();
        // The log reader strips the "Sender > " framing, so analyze sees only the body.
        let txt = "DT gate to UALX Camped";
        let r = analyze(txt, &s, &noships(), &noknown(), 1, "ch", "Frizank2");
        // DT/UALX are abbreviations that resolve to no character via ESI.
        let resolved = esi_resolve(&r.pilots, &[]);
        assert!(
            !resolved.iter().any(|p| p == "UALX" || p == "DT"),
            "unresolved system code became a pilot: {resolved:?}",
        );
        assert!(r.camp, "camped should set the camp keyword: {:?}", r.text);
    }

    /// Verbatim regressions for every intel line reported this session. Each test pins one exact
    /// line so a future refactor can't silently break it. Lines that already had dedicated coverage
    /// are noted (with the owning test) rather than duplicated; only the gaps are re-asserted here.
    mod session_regressions {
        use super::*;

        // Line 1 `v Ruston Shackleford B-3QPD` — covered by `stray_letter_before_name_with_code_system`
        //   (known-cache path: pilot "Ruston Shackleford", system B-3QPD, no gate). Not duplicated.
        // Line 2 `they are` / `back to` / `I'm tackled in Rancer` — covered by
        //   `common_phrases_not_parsed_as_pilots` (none become pilots; Rancer + tackle still parse).
        // Line 3 `cythe fleet issue tackled in Rancer` — covered by
        //   `fuzzy_typo_multiword_hull_is_a_ship_not_pilots` (ship "Scythe Fleet Issue"; no
        //   cythe/fleet/issue pilots).
        // Line 4 `willlin qiuxiaoye Micahel wu v Htguuu Htg-0 灵感级* 金鹏级*` — covered by
        //   `stray_letter_midrun_splits_pilot_list`.
        // Line 5 `MSKR-1 Htg-0 +5 gnosis 3x, Slasher, ESS 346mio` — covered by
        //   `htg0_is_a_pilot_mskr1_stays_a_system`.
        // Line 6 `bovine worm` — covered by `pilot_word_that_is_a_hull_is_not_also_a_ship`.
        // Line 9 punctuation guard — covered by `noise_punctuation_between_tokens_does_not_break_detection`
        //   (comma/asterisk split; apostrophe O'Brien + hyphen Htg-0 preserved).

        /// Line 7 (Task 1): a sub-phrase pilot repeated as its own token survives. "Tiffanbrill"
        /// appears once standalone and once inside the distinct "Tiffanbrill Dragon", so BOTH must
        /// show — alongside "Furry For Life", the three hulls, and the HL-VZX system.
        #[test]
        fn line7_repeated_subphrase_pilot_survives() {
            let s = systems_with(&[("hl-vzx", "HL-VZX", 30002, -0.4)]);
            let ships = ships_with(&[("Stabber", 622), ("Orthrus", 33470), ("Stiletto", 11198)]);
            // ESI/known names the cover confirms out of the over-glued blob.
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
            // Ships are detected directly.
            for hull in ["Stabber", "Orthrus", "Stiletto"] {
                assert!(r.ships.iter().any(|sh| sh.name == hull), "missing {hull}: {:?}", r.ships);
            }
            // The held name blob is split by the ESI cover (deterministic mirror), which frees the
            // HL-VZX system and surfaces all three distinct pilots.
            let reals = ["Furry For Life", "Tiffanbrill", "Tiffanbrill Dragon"];
            let (pilots, sysd, _gates) = resolve_report(&r, &reals, &s);
            for name in reals {
                assert!(
                    pilots.iter().any(|p| p.eq_ignore_ascii_case(name)),
                    "pilot {name:?} missing: {pilots:?}",
                );
            }
            assert_eq!(sysd, vec!["HL-VZX".to_string()], "system: {sysd:?}");

            // Direct proof of the Task-1 fix: the merge dedup (which calls drop_subphrase_pilots)
            // keeps both Tiffanbrill names when the report text carries two "Tiffanbrill" tokens.
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

        /// Line 8 (parse_isk): the exact ESS-context amounts reported this session. "ess 346mio" /
        /// "ess 300kk 5 min" also appear in `parse_isk_*` tests; "ess robbed 30m"=None likewise.
        /// Pinned verbatim here as one table so the ESS gating can't silently regress.
        #[test]
        fn line8_parse_isk_ess_amounts() {
            assert_eq!(parse_isk("ess robbed 30m", true), None, "'robbed' is not an amount");
            assert_eq!(parse_isk("ess 346mio", true), Some(346_000_000));
            assert_eq!(parse_isk("ess 77m", true), Some(77_000_000));
            assert_eq!(parse_isk("ess 300kk 5 min", true), Some(300_000_000));
        }
    }

    /// A full hull name is detected in ANY case (upper, lower, mixed) — typing the ship name
    /// always works, position/casing regardless.
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
        // Case-insensitive inside a sentence, too.
        let r = analyze("tackled a NAGA on the gate", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Naga"), "ships={:?}", r.ships);
    }

    /// Hulls typed after a decorated `+count` (hand-typed intel, single spaces) are all detected —
    /// the count must not glue the hulls into a pilot blob that hides them.
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

    /// A confirmed pilot glued to a leftover hull ("Bob Rifter", Bob known): the pilot wins AND the
    /// hull is still surfaced — the hull is never swallowed just because it sits next to a name.
    /// A hull that is genuinely part of the confirmed name stays masked; a fully-unconfirmed 2-word
    /// blob ("Sabre Smith") is still deferred to the ESI cover (no forced ship).
    #[test]
    fn hull_next_to_confirmed_pilot_still_detected() {
        let s = systems();
        let ships = ships_with(&[("Rifter", 587), ("Sabre", 22456), ("Worm", 17619)]);
        let known: std::collections::HashMap<String, i64> =
            [("bob".to_string(), 1i64), ("wolf e kristjansson".to_string(), 2i64)]
                .into_iter()
                .collect();
        // Confirmed "Bob" + leftover hull Rifter (either order) → both.
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
        // A hull that is PART of a confirmed multi-word name stays masked (not a ship).
        let ships2 = ships_with(&[("Wolf", 11371)]);
        let r = analyze("Wolf E Kristjansson nv", &s, &ships2, &known, 1, "ch", "x");
        assert!(r.ships.is_empty(), "confirmed-name hull word leaked: {:?}", r.ships);
        // Fully-unconfirmed 2-word blob is left alone (existing behaviour): no forced ship.
        let r = analyze("Sabre Smith in Rancer", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.is_empty(), "unconfirmed blob forced a ship: {:?}", r.ships);
    }

    /// Multi-word hull names are detected case-insensitively, including a single-word hull that is
    /// itself multi-word-adjacent.
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
        // A single-word hull in mixed case alongside a multi-word hull.
        let r = analyze("naga and a CYCLONE FLEET ISSUE", &s, &ships, &noknown(), 1, "ch", "x");
        assert!(r.ships.iter().any(|sh| sh.name == "Naga"), "ships={:?}", r.ships);
        assert!(
            r.ships.iter().any(|sh| sh.name == "Cyclone Fleet Issue"),
            "ships={:?}",
            r.ships
        );
    }

    /// A known MULTI-WORD system name ("Sanctified Vidette", a Drifter wormhole system) is
    /// classified as the SYSTEM, never a pilot — alone and surrounded by pilots/ships. Case
    /// insensitive. A normal 2-word pilot that is NOT a system still parses as a pilot.
    #[test]
    fn multiword_system_is_the_system_not_a_pilot() {
        let s = systems_with(&[
            ("sanctified vidette", "Sanctified Vidette", 31000123, -1.0),
            ("rancer", "Rancer", 1, 0.4),
        ]);
        let ships = ships_with(&[("Rifter", 587)]);
        // Alone: it's the system, no pilot.
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
        // Surrounded by a confirmed pilot and a ship: the system is still recognised, and neither
        // of its words becomes a pilot.
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

        // A normal 2-word pilot that is NOT a system still parses as a pilot.
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

    /// Reporter's line "FN0-QS  Ben Walker NV": a double-space paste of the system + a pilot with a
    /// trailing "NV" no-visual tag. The trailing intel tag must be stripped from the paste segment
    /// so the pilot "Ben Walker" resolves (not "Ben Walker NV", which matches no character), the NV
    /// tag still sets no_visual, and FN0-QS is the system. The single-space form parses the same.
    #[test]
    fn paste_segment_trailing_tag_stripped_ben_walker() {
        let s = systems_with(&[("fn0-qs", "FN0-QS", 30004111, -0.4)]);
        let known: std::collections::HashMap<String, i64> =
            [("ben walker".to_string(), 1i64)].into_iter().collect();
        for line in ["FN0-QS  Ben Walker NV", "FN0-QS Ben Walker NV", "FN0-QS  Ben Walker  NV"] {
            let r = analyze(line, &s, &noships(), &known, 1, "ch", "x");
            // Pilot resolves to "Ben Walker" — the NV tag is not glued into the name.
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
            // The freed NV tag is read as no-visual.
            assert!(r.no_visual, "{line:?}: no_visual not set: {:?}", r.text);
            // FN0-QS is the system.
            assert!(
                r.systems.iter().any(|d| d.name == "FN0-QS"),
                "{line:?}: system missing: {:?}",
                r.systems
            );
            // The ESI cover (unknown-pilot path) also lands on "Ben Walker".
            let (pilots, _sys, _g) = resolve_report(&r, &["Ben Walker"], &s);
            assert!(pilots.iter().any(|p| p == "Ben Walker"), "{line:?}: cover: {pilots:?}");
        }
        // trim helper directly: a trailing tag is stripped, a real name ending in a name-capable
        // word or an initial/number is kept.
        assert_eq!(trim_paste_location_tail("Ben Walker NV", &ships_with(&[])), "Ben Walker");
        assert_eq!(trim_paste_location_tail("Ben Walker nv", &ships_with(&[])), "Ben Walker");
        assert_eq!(trim_paste_location_tail("Clear Rain", &ships_with(&[])), "Clear Rain");
        assert_eq!(trim_paste_location_tail("Blue Skies", &ships_with(&[])), "Blue Skies");
        assert_eq!(trim_paste_location_tail("Lopatich R", &ships_with(&[])), "Lopatich R");
        assert_eq!(trim_paste_location_tail("Malcolm 41", &ships_with(&[])), "Malcolm 41");
    }
}
