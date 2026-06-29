//! Battle-report clustering (docs/DESIGN.md §7.2).
//!
//! Groups killmails ("engagements") into battles: two engagements belong to the
//! same battle if they are within `window` seconds AND `max_jumps` jumps of each
//! other (transitively — a battle chains across systems/time). Jump distance uses
//! the geo graph, which already includes configured jump bridges.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// 10 minutes between linked engagements.
pub const BATTLE_WINDOW_SECS: i64 = 600;
/// Up to 3 jumps (gates or configured bridges) between linked engagements.
pub const BATTLE_MAX_JUMPS: u32 = 3;
/// A lull of this many seconds (5 min) inside one cluster hard-splits it into separate battles:
/// activity that stops for this long and resumes is a fresh engagement, not the same fight.
pub const BATTLE_BREAK_SECS: i64 = 300;
/// Two segments merge as one running fight (a chase) only when their participant overlap is at
/// least this share of the smaller roster.
const CHASE_OVERLAP: f64 = 0.5;
/// A chase may bridge a lull up to this long (20 min) between its two nearest kills.
const MERGE_MAX_GAP: i64 = 1200;
/// A chase may span up to this many jumps between any pair of its systems.
const MERGE_MAX_JUMPS: u32 = 10;
/// A merged chase may not span more than this long end-to-end (2 h) — beyond it, two bursts that
/// share pilots are separate fights, not one running engagement.
const MERGE_MAX_SPAN: i64 = 7200;
/// A "dense core" / real burst is at least this many kills; segments smaller than this are strays.
const DENSE_MIN: usize = 3;

/// Manual clustering overrides applied before automatic grouping. Plumbed through the pipeline so
/// later phases can let the user correct a battle's boundaries; Phase 1 always passes the default
/// (empty) set, so the behaviour is fully automatic.
#[derive(Default, Clone)]
pub struct Overrides {
    /// kill_id -> group tag: kills sharing a tag are forced into one battle and never split or
    /// merged across tags.
    pub tag: HashMap<i64, i64>,
    /// kill_ids to drop entirely before clustering.
    pub excluded: HashSet<i64>,
    /// (kill_id, char_id) attacker entries to scrub from a kill before clustering.
    pub scrubs: HashSet<(i64, i64)>,
}

#[allow(dead_code)] // Faction is for future faction-warfare kills
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartyKind {
    Alliance,
    Corporation,
    Character,
    Faction,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Party {
    /// Entity id — for future zKill/Dotlan links.
    #[allow(dead_code)]
    pub id: i64,
    pub name: String,
    /// Alliance/corp/character — for future per-party icons.
    #[allow(dead_code)]
    pub kind: PartyKind,
}

/// One attacker on a killmail: their side-identity party, the ship they flew, and whether
/// they landed the final blow.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attacker {
    /// Side-inference identity (alliance/corp/character collapsed, like a victim's party).
    pub party: Party,
    /// Character id; 0 for NPC / structure attackers (used to dedup surviving pilots).
    pub char_id: i64,
    /// Ship flown (type id); 0 when unknown (e.g. an NPC/structure with no ship).
    pub ship: i64,
    /// Best-available pilot label (character > corp > alliance name), for the last-hit line.
    pub pilot: String,
    pub final_blow: bool,
}

/// Capsule (pod) ship type ids: regular and the Genolution variant. A pod kill is folded
/// into its pilot's ship kill rather than shown separately.
pub const POD_TYPES: [i64; 2] = [670, 33328];

/// One killmail: a victim destroyed by attackers, in a system, at a time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Engagement {
    pub kill_id: i64,
    pub time: i64,
    pub system_id: i64,
    pub system_name: String,
    pub security: f64,
    pub victim: Party,
    /// Victim character id; 0 if none (e.g. a structure). Matches a pod kill to its ship kill.
    pub victim_char: i64,
    /// Victim pilot display name (character, else corp/alliance).
    pub victim_pilot: String,
    /// The destroyed ship / structure type id (for the lost-hull badge).
    pub victim_ship: i64,
    pub attackers: Vec<Attacker>,
    pub isk: f64,
    /// Whether this kill was genuinely in the watched area at ingest (within ANCHOR_JUMPS of
    /// intel or the active character, or matched by a custom Include rule). Kills just outside
    /// the area are still buffered as battle *candidates* (`anchored = false`); a battle is only
    /// surfaced if it contains at least one anchored kill, so a fight that touches the watched
    /// area is recorded whole — including its out-of-range kills — without surfacing battles that
    /// are entirely elsewhere. Defaults to true so engagements persisted before this field
    /// (all of which were in-area) still count as anchors.
    #[serde(default = "default_true")]
    pub anchored: bool,
}

fn default_true() -> bool {
    true
}

impl Engagement {
    /// The killer to credit: the attacker party that appears most (the dominant entity on the
    /// kill), falling back to the first attacker.
    #[allow(dead_code)] // kept for side-of-killer attribution
    pub fn killer(&self) -> Option<&Party> {
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for a in &self.attackers {
            if a.party.id != 0 {
                *counts.entry(a.party.id).or_default() += 1;
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .and_then(|(id, _)| self.attackers.iter().find(|a| a.party.id == id))
            .or_else(|| self.attackers.first())
            .map(|a| &a.party)
    }
}

/// One side of a battle: allied parties (fought together / not significantly hostile to each
/// other), optionally a recognised coalition.
#[derive(Clone, Debug)]
pub struct Side {
    /// Member parties, most-involved first.
    pub parties: Vec<Party>,
    /// Coalition name when the members map to one (e.g. "The Imperium").
    pub coalition: Option<String>,
    /// Kills scored by this side.
    pub kills: u32,
    /// Ships lost by this side.
    pub losses: u32,
    /// ISK destroyed *from* this side (its losses' value).
    pub isk_lost: f64,
    /// ISK this side destroyed (value of enemy hulls it scored on).
    pub isk_destroyed: f64,
}

impl Side {
    /// ISK efficiency: share of the ISK exchanged with this side that it dealt rather than took,
    /// as a percentage (the zKill-style stat). `None` when no ISK was exchanged.
    pub fn isk_efficiency(&self) -> Option<f64> {
        let total = self.isk_destroyed + self.isk_lost;
        (total > 0.0).then(|| self.isk_destroyed / total * 100.0)
    }
}

#[derive(Clone, Debug)]
pub struct Battle {
    pub engagements: Vec<Engagement>,
    pub start: i64,
    pub end: i64,
    /// Systems involved: (id, name, security).
    pub systems: Vec<(i64, String, f64)>,
    /// Belligerent sides, largest first (usually 2).
    pub sides: Vec<Side>,
    pub kills: usize,
    pub isk: f64,
    /// The automatic clustering is unsure where this battle's boundaries are (e.g. a near-threshold
    /// lull or a single bridge kill joining two bursts); `suggested_splits` then holds candidate
    /// split times for a later phase to surface.
    pub ambiguous: bool,
    /// Candidate split times (unix seconds), at most 3, sorted; empty when unambiguous.
    pub suggested_splits: Vec<i64>,
}

/// Extra info for a destroyed participant: its killmail and hull value, plus the value of a
/// pod that was killed with it (folded in), if any.
#[derive(Clone, Debug)]
pub struct Lost {
    pub kill_id: i64,
    pub value: f64,
    /// Value of the pilot's pod, killed alongside the ship (0 if the pod survived).
    pub pod_value: f64,
    /// Type id of that pod (capsule variant), so the indicator shows the right icon.
    pub pod_ship: i64,
}

/// One ship that took part on a side: a pilot in a hull, either destroyed (`lost`) or survived.
#[derive(Clone, Debug)]
pub struct Participant {
    /// Pilot character id (0 if none), to cross-reference involvement on hover.
    pub char_id: i64,
    pub party: Party,
    pub pilot: String,
    pub ship: i64,
    /// Set when the ship was destroyed.
    pub lost: Option<Lost>,
}

/// Cross-references for the hover highlight, keyed by character id.
#[derive(Default)]
pub struct Involvement {
    /// attacker -> victims they helped kill (were on the killmail of).
    pub killed: HashMap<i64, std::collections::HashSet<i64>>,
    /// kill_id -> the attacker char_ids that scored it (for the red border on a hovered loss).
    pub attackers: HashMap<i64, std::collections::HashSet<i64>>,
}

impl Battle {
    /// True if any engagement was in the watched area at ingest. Battles with no anchored
    /// engagement are entirely outside the watched area (only buffered as cluster candidates)
    /// and must not be surfaced.
    pub fn is_anchored(&self) -> bool {
        self.engagements.iter().any(|e| e.anchored)
    }

    /// A real battle has at least two belligerent sides. A single inferred side means the kills
    /// were friendly fire, or a gank/loss with only NPC or structure attackers (NPCs are never a
    /// side) — not a fight worth a report.
    pub fn is_two_sided(&self) -> bool {
        self.sides.len() >= 2
    }

    /// Build the hover cross-reference maps from the engagements.
    pub fn involvement(&self) -> Involvement {
        let mut inv = Involvement::default();
        for e in &self.engagements {
            let victim = e.victim_char;
            let mut atk: std::collections::HashSet<i64> = std::collections::HashSet::new();
            for a in &e.attackers {
                if a.char_id == 0 {
                    continue;
                }
                atk.insert(a.char_id);
                if victim != 0 {
                    inv.killed.entry(a.char_id).or_default().insert(victim);
                }
            }
            inv.attackers.insert(e.kill_id, atk);
        }
        inv
    }
}

impl Battle {
    /// Which side a party belongs to (match by id, or by name when the id is unknown).
    pub fn side_of(&self, p: &Party) -> Option<usize> {
        self.sides.iter().position(|s| {
            s.parties.iter().any(|q| if p.id != 0 { q.id == p.id } else { q.name == p.name })
        })
    }

    /// Whether this battle matches a free-text query (case-insensitive substring over the
    /// systems, side coalitions/parties, and every pilot name). `q` must already be lower-cased.
    pub fn matches(&self, q: &str) -> bool {
        if q.is_empty() {
            return true;
        }
        let hit = |s: &str| s.to_lowercase().contains(q);
        if self.systems.iter().any(|(_, name, _)| hit(name)) {
            return true;
        }
        for side in &self.sides {
            if side.coalition.as_deref().is_some_and(hit) {
                return true;
            }
            if side.parties.iter().any(|p| hit(&p.name)) {
                return true;
            }
        }
        self.engagements.iter().any(|e| {
            hit(&e.victim_pilot) || e.attackers.iter().any(|a| hit(&a.pilot))
        })
    }

    /// Side `i`'s roster: every participating ship, grouped by hull type. Within a hull, the
    /// destroyed ships come first (highest value), then the survivors. Pods are folded into
    /// their pilot's ship loss; surviving pilots are deduped and exclude anyone who died.
    pub fn roster(&self, i: usize) -> Vec<Participant> {
        struct Loss {
            kill_id: i64,
            char_id: i64,
            pilot: String,
            party: Party,
            ship: i64,
            value: f64,
            pod_value: f64,
            pod_ship: i64,
        }
        let mut ships: Vec<Loss> = Vec::new();
        let mut pods: Vec<Loss> = Vec::new();
        for e in &self.engagements {
            if self.side_of(&e.victim) != Some(i) {
                continue;
            }
            let loss = Loss {
                kill_id: e.kill_id,
                char_id: e.victim_char,
                pilot: e.victim_pilot.clone(),
                party: e.victim.clone(),
                ship: e.victim_ship,
                value: e.isk,
                pod_value: 0.0,
                pod_ship: 0,
            };
            if POD_TYPES.contains(&e.victim_ship) {
                pods.push(loss);
            } else {
                ships.push(loss);
            }
        }
        let same = |a: &Loss, b: &Loss| {
            if a.char_id != 0 && b.char_id != 0 { a.char_id == b.char_id } else { a.pilot == b.pilot }
        };
        for pod in pods {
            // Fold the pod into its pilot's ship loss as a separate value, so the row can show a
            // "+ pod" indicator; a pod with no ship loss stays as its own row.
            match ships.iter_mut().find(|s| same(s, &pod)) {
                Some(ship) => {
                    ship.pod_value += pod.value;
                    ship.pod_ship = pod.ship; // the actual capsule variant
                }
                None => ships.push(pod),
            }
        }

        // Pilots who died on this side are not also listed as survivors.
        let dead_ids: BTreeSet<i64> = ships.iter().map(|l| l.char_id).filter(|&c| c != 0).collect();
        let dead_names: BTreeSet<String> = ships.iter().map(|l| l.pilot.clone()).collect();

        let mut parts: Vec<Participant> = ships
            .into_iter()
            .map(|l| Participant {
                char_id: l.char_id,
                party: l.party,
                pilot: l.pilot,
                ship: l.ship,
                lost: Some(Lost {
                    kill_id: l.kill_id,
                    value: l.value,
                    pod_value: l.pod_value,
                    pod_ship: l.pod_ship,
                }),
            })
            .collect();

        let mut seen: BTreeSet<i64> = BTreeSet::new();
        for e in &self.engagements {
            for a in &e.attackers {
                if a.char_id == 0 || self.side_of(&a.party) != Some(i) {
                    continue; // NPC/structure, or fighting for another side
                }
                if dead_ids.contains(&a.char_id) || dead_names.contains(&a.pilot) {
                    continue;
                }
                if seen.insert(a.char_id) {
                    parts.push(Participant {
                        char_id: a.char_id,
                        party: a.party.clone(),
                        pilot: a.pilot.clone(),
                        ship: a.ship,
                        lost: None,
                    });
                }
            }
        }

        // Highest-value kills first (ship + pod), descending; survivors (no loss) sink to the
        // bottom, then same hulls and pilots stay together for a stable order.
        let total = |p: &Participant| p.lost.as_ref().map_or(0.0, |l| l.value + l.pod_value);
        parts.sort_by(|a, b| {
            total(b)
                .total_cmp(&total(a))
                .then(b.lost.is_some().cmp(&a.lost.is_some()))
                .then(a.ship.cmp(&b.ship))
                .then(a.pilot.cmp(&b.pilot))
        });
        parts
    }
}

/// Cluster engagements into battles. `dist(a, b)` is the jump distance between two
/// systems (None if too far / unreachable).
pub fn cluster(
    engagements: &[Engagement],
    window: i64,
    max_jumps: u32,
    break_gap: i64,
    overrides: &Overrides,
    dist: impl Fn(i64, i64) -> Option<u32>,
) -> Vec<Battle> {
    let (filtered, groups) =
        partition_battles(engagements, window, max_jumps, break_gap, overrides, &dist);
    let mut battles: Vec<Battle> = groups
        .into_iter()
        .map(|idxs| build_battle(idxs.iter().map(|&i| filtered[i].clone()).collect(), break_gap))
        .collect();
    battles.sort_by(|a, b| b.end.cmp(&a.end)); // newest first
    battles
}

/// Like [`cluster`], but reuses a battle from `cache` whenever its exact engagement set (kill ids)
/// is unchanged — so a re-cluster after a handful of new kills only rebuilds the battles that
/// actually changed instead of re-inferring every side every time. `cache` is replaced with the
/// new generation (stale battles drop out).
pub fn cluster_cached(
    engagements: &[Engagement],
    window: i64,
    max_jumps: u32,
    break_gap: i64,
    overrides: &Overrides,
    dist: impl Fn(i64, i64) -> Option<u32>,
    cache: &mut HashMap<u64, Battle>,
) -> Vec<Battle> {
    use std::hash::{Hash, Hasher};
    let (filtered, groups) =
        partition_battles(engagements, window, max_jumps, break_gap, overrides, &dist);
    let mut next: HashMap<u64, Battle> = HashMap::new();
    let mut battles: Vec<Battle> = groups
        .into_iter()
        .map(|idxs| {
            let mut kids: Vec<i64> = idxs.iter().map(|&i| filtered[i].kill_id).collect();
            kids.sort_unstable();
            let mut h = std::collections::hash_map::DefaultHasher::new();
            kids.hash(&mut h);
            let sig = h.finish();
            let b = cache.get(&sig).cloned().unwrap_or_else(|| {
                build_battle(idxs.iter().map(|&i| filtered[i].clone()).collect(), break_gap)
            });
            next.insert(sig, b.clone());
            b
        })
        .collect();
    *cache = next;
    battles.sort_by(|a, b| b.end.cmp(&a.end)); // newest first
    battles
}

/// The clustering pipeline: apply `overrides` (exclude kills, scrub attackers), partition the
/// surviving engagements into battle index-groups, segment long lulls, and re-merge running chases.
/// Returns the filtered engagements together with the final index-groups into them.
fn partition_battles(
    engagements: &[Engagement],
    window: i64,
    max_jumps: u32,
    break_gap: i64,
    overrides: &Overrides,
    dist: &impl Fn(i64, i64) -> Option<u32>,
) -> (Vec<Engagement>, Vec<Vec<usize>>) {
    // (a) Apply overrides: drop excluded kills and scrub flagged attacker entries.
    let filtered: Vec<Engagement> = engagements
        .iter()
        .filter(|e| !overrides.excluded.contains(&e.kill_id))
        .map(|e| {
            let mut e = e.clone();
            if !overrides.scrubs.is_empty() {
                let kid = e.kill_id;
                e.attackers.retain(|a| !overrides.scrubs.contains(&(kid, a.char_id)));
            }
            e
        })
        .collect();

    // (b) Proximity/participant grouping (with manual tags forcing/forbidding merges).
    let groups = group_indices(&filtered, window, max_jumps, &overrides.tag, dist);

    // (c) Tagged groups pass through whole; untagged groups are segmented at lulls.
    let mut tagged_groups: Vec<Vec<usize>> = Vec::new();
    let mut untagged_segments: Vec<Vec<usize>> = Vec::new();
    for group in groups {
        let first_tag = overrides.tag.get(&filtered[group[0]].kill_id).copied();
        let all_tagged = first_tag.is_some()
            && group.iter().all(|&i| overrides.tag.get(&filtered[i].kill_id).copied() == first_tag);
        if all_tagged {
            tagged_groups.push(group);
        } else {
            untagged_segments.extend(segment_indices(&filtered, group, break_gap));
        }
    }

    // (d) Re-merge running chases over the untagged segments only; tagged groups are untouched.
    let mut final_groups = merge_chases(&filtered, untagged_segments, dist);
    final_groups.extend(tagged_groups);
    (filtered, final_groups)
}

/// Partition engagement indices into battles: two engagements chain when they share a participant
/// and are within `window` seconds and `max_jumps` jumps (transitively).
fn group_indices(
    engagements: &[Engagement],
    window: i64,
    max_jumps: u32,
    tags: &HashMap<i64, i64>,
    dist: &impl Fn(i64, i64) -> Option<u32>,
) -> Vec<Vec<usize>> {
    let n = engagements.len();
    // Belligerent ids per engagement (victim + attackers). Two engagements only chain into the
    // same battle if they share a participant — otherwise unrelated fights close in space and
    // time (around a hub like Jita) get merged into one report. Chaining is transitive, so a
    // battle still holds together through a shared participant on any linking engagement.
    let parties: Vec<std::collections::HashSet<i64>> = engagements
        .iter()
        .map(|e| {
            let mut s: std::collections::HashSet<i64> = std::collections::HashSet::new();
            if e.victim.id != 0 {
                s.insert(e.victim.id);
            }
            for a in &e.attackers {
                if a.party.id != 0 {
                    s.insert(a.party.id);
                }
            }
            s
        })
        .collect();
    let mut uf = UnionFind::new(n);

    // Manual tags: every kill carrying the same tag is a must-link (forced into one component,
    // regardless of time/space), and differing tags are a cannot-link (never merged). Track each
    // component's tag so the proximity pass below can refuse a merge that would cross tags.
    let elem_tag: Vec<Option<i64>> = engagements.iter().map(|e| tags.get(&e.kill_id).copied()).collect();
    let mut by_tag: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, t) in elem_tag.iter().enumerate() {
        if let Some(t) = t {
            by_tag.entry(*t).or_default().push(i);
        }
    }
    for members in by_tag.values() {
        for w in members.windows(2) {
            uf.union(w[0], w[1]);
        }
    }
    // Component root -> tag (kept current via `find` after each union).
    let mut root_tag: HashMap<usize, i64> = HashMap::new();
    for (t, members) in &by_tag {
        root_tag.insert(uf.find(members[0]), *t);
    }

    // Compare only engagements within `window` of each other: visit in time order and slide a
    // window forward, breaking as soon as a pair is too far apart. Engagements far apart in time
    // (the bulk of full battle history) are never compared, so this is ~O(n × kills-per-window)
    // instead of O(n²).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| engagements[i].time);
    for oi in 0..n {
        let i = order[oi];
        for &j in order.iter().skip(oi + 1) {
            // Sorted by time, so the gap only grows — once it reaches the window, stop.
            if engagements[j].time - engagements[i].time >= window {
                break;
            }
            let (ri, rj) = (uf.find(i), uf.find(j));
            // Shared participant is the cheap, deciding test — apply it before the (potentially
            // expensive) jump-distance BFS, and skip pairs already in the same battle.
            if parties[i].is_disjoint(&parties[j]) || ri == rj {
                continue;
            }
            // Cannot-link: two tagged components with different tags never merge.
            if let (Some(ta), Some(tb)) = (root_tag.get(&ri).copied(), root_tag.get(&rj).copied()) {
                if ta != tb {
                    continue;
                }
            }
            let (a, b) = (&engagements[i], &engagements[j]);
            let close = a.system_id == b.system_id
                || dist(a.system_id, b.system_id).is_some_and(|d| d <= max_jumps);
            if close {
                uf.union(i, j);
                // The merged component inherits whichever side's tag is set.
                let merged_tag = root_tag.get(&ri).copied().or_else(|| root_tag.get(&rj).copied());
                if let Some(t) = merged_tag {
                    root_tag.insert(uf.find(i), t);
                }
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        groups.entry(uf.find(i)).or_default().push(i);
    }
    groups.into_values().collect()
}

/// Build a one-off `Battle` from an arbitrary engagement subset (for the split-preview UI):
/// re-infers sides + tallies exactly as the clusterer does.
pub fn preview_battle(engs: Vec<Engagement>, break_gap: i64) -> Battle {
    build_battle(engs, break_gap)
}

fn build_battle(mut engs: Vec<Engagement>, break_gap: i64) -> Battle {
    engs.sort_by_key(|e| e.time);
    let start = engs.first().map_or(0, |e| e.time);
    let end = engs.last().map_or(0, |e| e.time);

    let mut systems: BTreeMap<i64, (String, f64)> = BTreeMap::new();
    let mut isk = 0.0;
    for e in &engs {
        systems.insert(e.system_id, (e.system_name.clone(), e.security));
        isk += e.isk;
    }

    let (ambiguous, suggested_splits) = battle_ambiguity(&engs, break_gap);
    Battle {
        kills: engs.len(),
        isk,
        systems: systems
            .into_iter()
            .map(|(id, (name, sec))| (id, name, sec))
            .collect(),
        sides: infer_sides(&engs),
        start,
        end,
        ambiguous,
        suggested_splits,
        engagements: engs,
    }
}

/// Distinct character ids on a set of engagements (victim + attackers), ignoring 0 (NPC/structure).
fn segment_chars(engs: &[Engagement], idxs: &[usize]) -> HashSet<i64> {
    let mut s: HashSet<i64> = HashSet::new();
    for &i in idxs {
        if engs[i].victim_char != 0 {
            s.insert(engs[i].victim_char);
        }
        for a in &engs[i].attackers {
            if a.char_id != 0 {
                s.insert(a.char_id);
            }
        }
    }
    s
}

/// The earliest and latest kill time over a set of engagement indices.
fn time_bounds(engs: &[Engagement], idxs: &[usize]) -> (i64, i64) {
    let mut lo = i64::MAX;
    let mut hi = i64::MIN;
    for &i in idxs {
        lo = lo.min(engs[i].time);
        hi = hi.max(engs[i].time);
    }
    (lo, hi)
}

/// Split one cluster's engagement indices into separate battles at lulls. First a hard split at any
/// consecutive gap `>= break_gap`; then, within a long hard segment, a density-valley refinement
/// that separates two dense bursts joined only by a sparse string of stray kills.
fn segment_indices(engs: &[Engagement], mut idxs: Vec<usize>, break_gap: i64) -> Vec<Vec<usize>> {
    idxs.sort_by_key(|&i| engs[i].time);
    // (a) Hard split at any lull >= break_gap.
    let mut hard: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for &i in &idxs {
        if let Some(&last) = cur.last() {
            if engs[i].time - engs[last].time >= break_gap {
                hard.push(std::mem::take(&mut cur));
            }
        }
        cur.push(i);
    }
    if !cur.is_empty() {
        hard.push(cur);
    }
    // (b) Density-valley refinement of each hard segment.
    let mut out: Vec<Vec<usize>> = Vec::new();
    for seg in hard {
        refine_density(engs, seg, break_gap, &mut out);
    }
    out
}

/// Recursively split a hard segment where two dense cores are separated by a sparse valley.
fn refine_density(engs: &[Engagement], seg: Vec<usize>, break_gap: i64, out: &mut Vec<Vec<usize>>) {
    if seg.len() < 2 * DENSE_MIN {
        out.push(seg);
        return;
    }
    match valley_split(engs, &seg, break_gap) {
        Some(k) => {
            let mut left = seg;
            let right = left.split_off(k);
            refine_density(engs, left, break_gap, out);
            refine_density(engs, right, break_gap, out);
        }
        None => out.push(seg),
    }
}

/// Find a split index in a (time-sorted) segment where two dense bursts are joined only by a sparse
/// valley of stray kills. Buckets the segment into `break_gap`-wide bins; a valley is a maximal run
/// of low bins (`<= 1` kill) whose strays span `>= break_gap`, flanked on each side by a dense core
/// (a contiguous run of busy bins summing `>= DENSE_MIN`). Splits at the middle of the valley so the
/// strays attach to the nearer burst. Returns None when there is no such valley.
fn valley_split(engs: &[Engagement], seg: &[usize], break_gap: i64) -> Option<usize> {
    let start = engs[seg[0]].time;
    let to_bin = |t: i64| ((t - start) / break_gap) as usize;
    let nbins = to_bin(engs[*seg.last().unwrap()].time) + 1;
    if nbins < 3 {
        return None; // need core | valley | core
    }
    let mut counts = vec![0usize; nbins];
    for &i in seg {
        counts[to_bin(engs[i].time)] += 1;
    }
    // Scan for an interior run of low bins.
    let mut b = 0;
    while b < nbins {
        if counts[b] > 1 {
            b += 1;
            continue;
        }
        let lo = b;
        while b < nbins && counts[b] <= 1 {
            b += 1;
        }
        let hi = b - 1; // low run is bins [lo, hi]
        if lo == 0 || hi == nbins - 1 {
            continue; // not interior — no burst on one side
        }
        // Dense core immediately left of the valley: walk back over busy (>= 2) bins.
        let mut core_l = 0usize;
        let mut p = lo;
        while p > 0 && counts[p - 1] >= 2 {
            p -= 1;
            core_l += counts[p];
        }
        // Dense core immediately right of the valley.
        let mut core_r = 0usize;
        let mut q = hi + 1;
        while q < nbins && counts[q] >= 2 {
            core_r += counts[q];
            q += 1;
        }
        if core_l < DENSE_MIN || core_r < DENSE_MIN {
            continue;
        }
        // The valley's strays must actually span a lull of >= break_gap.
        let strays: Vec<usize> = seg
            .iter()
            .copied()
            .filter(|&i| (lo..=hi).contains(&to_bin(engs[i].time)))
            .collect();
        let (sl, sh) = time_bounds(engs, &strays);
        if sh - sl < break_gap {
            continue;
        }
        // Split at the middle bin boundary of the valley; strays before it attach left, after right.
        let cut_bin = lo + (hi - lo + 1) / 2;
        let k = seg.iter().position(|&i| to_bin(engs[i].time) >= cut_bin)?;
        if k == 0 || k == seg.len() {
            continue;
        }
        return Some(k);
    }
    None
}

/// Re-merge segments that are one running fight (a chase): the same pilots pursued across a short
/// lull and a few jumps. Greedy single pass over segments in time order — each segment either folds
/// into an already-accumulated segment it chases with, or starts a new one. Distinct pilots (e.g.
/// separate skirmishes by the same alliance) never merge, since the overlap test is character-level.
fn merge_chases(
    engs: &[Engagement],
    segments: Vec<Vec<usize>>,
    dist: &impl Fn(i64, i64) -> Option<u32>,
) -> Vec<Vec<usize>> {
    let mut segs = segments;
    segs.sort_by_key(|s| time_bounds(engs, s).0);
    let mut acc: Vec<Vec<usize>> = Vec::new();
    for seg in segs {
        let mut merged = false;
        for a in acc.iter_mut() {
            if chases(engs, a, &seg, dist) {
                a.extend(seg.iter().copied());
                merged = true;
                break;
            }
        }
        if !merged {
            acc.push(seg);
        }
    }
    acc
}

/// Whether segment `b` continues the same fight as accumulated segment `a`: enough shared pilots,
/// within the chase time gap and jump range, and without overrunning the span cap.
fn chases(
    engs: &[Engagement],
    a: &[usize],
    b: &[usize],
    dist: &impl Fn(i64, i64) -> Option<u32>,
) -> bool {
    let ca = segment_chars(engs, a);
    let cb = segment_chars(engs, b);
    let shared = ca.intersection(&cb).count();
    let min_len = ca.len().min(cb.len());
    if shared < 3 || min_len == 0 || (shared as f64) / (min_len as f64) < CHASE_OVERLAP {
        return false;
    }
    let (amin, amax) = time_bounds(engs, a);
    let (bmin, bmax) = time_bounds(engs, b);
    let gap = if amax < bmin {
        bmin - amax
    } else if bmax < amin {
        amin - bmax
    } else {
        0
    };
    if gap >= MERGE_MAX_GAP {
        return false;
    }
    if amax.max(bmax) - amin.min(bmin) > MERGE_MAX_SPAN {
        return false;
    }
    let sa: HashSet<i64> = a.iter().map(|&i| engs[i].system_id).collect();
    let sb: HashSet<i64> = b.iter().map(|&i| engs[i].system_id).collect();
    sa.iter().any(|&x| {
        sb.iter().any(|&y| x == y || dist(x, y).is_some_and(|d| d <= MERGE_MAX_JUMPS))
    })
}

/// Detect whether a battle's boundaries are ambiguous, returning candidate split times (<= 3,
/// sorted). `engs` must be time-sorted. Flags: (1) a near-threshold lull; (2) a single bridge kill
/// joining two dense halves; (3) a finer re-segmentation that finds multiple dense sub-battles.
fn battle_ambiguity(engs: &[Engagement], break_gap: i64) -> (bool, Vec<i64>) {
    let n = engs.len();
    let mut splits: Vec<i64> = Vec::new();

    // (1) Largest internal lull G with 0.6*break_gap <= G < break_gap — close to a hard split.
    if n >= 2 {
        let mut max_gap = 0i64;
        let mut at = 0usize;
        for m in 1..n {
            let g = engs[m].time - engs[m - 1].time;
            if g > max_gap {
                max_gap = g;
                at = m;
            }
        }
        if 0.6 * (break_gap as f64) <= max_gap as f64 && max_gap < break_gap {
            splits.push(engs[at].time);
        }
    }

    // (2) A single bridge kill: no participant spans strictly across it, and both sides are dense.
    if n >= 2 * DENSE_MIN {
        let mut first: HashMap<i64, usize> = HashMap::new();
        let mut last: HashMap<i64, usize> = HashMap::new();
        for (k, e) in engs.iter().enumerate() {
            let mut ids: Vec<i64> = Vec::new();
            if e.victim.id != 0 {
                ids.push(e.victim.id);
            }
            for a in &e.attackers {
                if a.party.id != 0 {
                    ids.push(a.party.id);
                }
            }
            for id in ids {
                first.entry(id).or_insert(k);
                last.insert(id, k);
            }
        }
        for k in 0..n {
            let spans = first
                .iter()
                .any(|(id, &f)| f < k && last.get(id).copied().unwrap_or(0) > k);
            if !spans && k >= DENSE_MIN && n - 1 - k >= DENSE_MIN {
                splits.push(engs[k].time);
            }
        }
    }

    // (3) Re-segment at half the lull: >= 2 dense sub-segments means there are interior boundaries.
    {
        let subs = segment_indices(engs, (0..n).collect(), break_gap / 2);
        if subs.iter().filter(|s| s.len() >= DENSE_MIN).count() >= 2 {
            let mut subs = subs;
            subs.sort_by_key(|s| time_bounds(engs, s).0);
            for w in subs.windows(2) {
                if w[0].len() >= DENSE_MIN && w[1].len() >= DENSE_MIN {
                    splits.push(time_bounds(engs, &w[1]).0);
                }
            }
        }
    }

    splits.sort_unstable();
    splits.dedup();
    splits.truncate(3);
    (!splits.is_empty(), splits)
}


/// Partition the belligerents into sides. Allies are parties that fought together and didn't
/// significantly shoot each other; sides are then merged by shared aggression ("we both fought
/// X" → same side), so accidental/occasional friendly fire doesn't split a coalition.
fn infer_sides(engs: &[Engagement]) -> Vec<Side> {
    // Party identity: alliance/corp/character id (unknowns keyed by name).
    let key = |p: &Party| if p.id != 0 { format!("#{}", p.id) } else { format!("n:{}", p.name) };
    let mut party_by_key: HashMap<String, Party> = HashMap::new();
    for e in engs {
        party_by_key.entry(key(&e.victim)).or_insert_with(|| e.victim.clone());
        for a in &e.attackers {
            party_by_key.entry(key(&a.party)).or_insert_with(|| a.party.clone());
        }
    }
    let keys: Vec<String> = party_by_key.keys().cloned().collect();
    let idx: HashMap<String, usize> =
        keys.iter().cloned().enumerate().map(|(i, k)| (k, i)).collect();
    let n = keys.len();

    // attacker -> victim kill counts, and co-attacker pair counts.
    let mut hostility: HashMap<(usize, usize), u32> = HashMap::new();
    // How many killmails each pair co-attacked on. Genuine allies fight together repeatedly; a
    // cross-side kill-steal (both landing on one third-party victim) co-attacks only once or twice,
    // so the count lets the real coalitions coalesce before any weak bridge is even considered.
    let mut coattack: HashMap<(usize, usize), u32> = HashMap::new();
    for e in engs {
        let v = idx[&key(&e.victim)];
        let mut atk: Vec<usize> = e.attackers.iter().map(|a| idx[&key(&a.party)]).collect();
        atk.sort_unstable();
        atk.dedup();
        for &a in &atk {
            if a != v {
                *hostility.entry((a, v)).or_default() += 1;
            }
        }
        for i in 0..atk.len() {
            for j in (i + 1)..atk.len() {
                *coattack.entry((atk[i], atk[j])).or_default() += 1;
            }
        }
    }
    let mutual = |a: usize, b: usize| {
        hostility.get(&(a, b)).copied().unwrap_or(0) + hostility.get(&(b, a)).copied().unwrap_or(0)
    };
    // Partition into sides by AGGLOMERATIVE NET ALLIANCE. Two parties belong together when they
    // fought *together* at least as much as they fought *each other*: the pairwise score is
    // (co-attacks − mutual kills). Repeatedly merge the components with the highest score while it
    // stays ≥ 0; stop once every remaining pair is net-hostile. Because the strongest allies
    // coalesce first, a stray cross-side kill-steal (one shared killmail) can never bridge two
    // coalitions — at the coalition level their mutual hostility dwarfs the lone co-attack, so the
    // score is deeply negative and they stay apart. A near-neutral straggler (only a victim, say)
    // folds into the side it is not hostile to rather than splintering into its own "side".
    let net_pair = |a: usize, b: usize| -> i64 {
        let (lo, hi) = (a.min(b), a.max(b));
        coattack.get(&(lo, hi)).copied().unwrap_or(0) as i64 - mutual(a, b) as i64
    };
    let mut net = vec![vec![0i64; n]; n];
    for a in 0..n {
        for b in (a + 1)..n {
            let v = net_pair(a, b);
            net[a][b] = v;
            net[b][a] = v;
        }
    }
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut alive = vec![true; n];
    loop {
        let mut best: Option<(usize, usize)> = None;
        let mut best_net = -1i64; // accept the largest score that is ≥ 0
        for i in 0..n {
            if !alive[i] {
                continue;
            }
            for j in (i + 1)..n {
                if alive[j] && net[i][j] >= 0 && net[i][j] > best_net {
                    best_net = net[i][j];
                    best = Some((i, j));
                }
            }
        }
        let Some((i, j)) = best else { break };
        let moved = std::mem::take(&mut members[j]);
        members[i].extend(moved);
        alive[j] = false;
        for k in 0..n {
            if alive[k] && k != i {
                net[i][k] += net[j][k];
                net[k][i] = net[i][k];
            }
        }
    }
    // Component representative per party (the surviving index of its component).
    let mut root = vec![0usize; n];
    for i in 0..n {
        if alive[i] {
            for &m in &members[i] {
                root[m] = i;
            }
        }
    }

    #[derive(Default)]
    struct Agg {
        members: BTreeSet<usize>,
        involve: HashMap<usize, u32>,
        kills: u32,
        losses: u32,
        isk_lost: f64,
        isk_destroyed: f64,
    }
    let mut sides: HashMap<usize, Agg> = HashMap::new();
    for i in 0..n {
        sides.entry(root[i]).or_default().members.insert(i);
    }
    for e in engs {
        let v = idx[&key(&e.victim)];
        let s = sides.get_mut(&root[v]).unwrap();
        s.losses += 1;
        s.isk_lost += e.isk;
        *s.involve.entry(v).or_default() += 1;
        let mut scored: BTreeSet<usize> = BTreeSet::new();
        for a in &e.attackers {
            let ai = idx[&key(&a.party)];
            *sides.get_mut(&root[ai]).unwrap().involve.entry(ai).or_default() += 1;
            // Only credit a kill / destroyed-ISK to a side that shot an *enemy*, not its own
            // member (friendly fire, or an NPC-dealt blow with a same-side assist). Otherwise a
            // side is credited for destroying its own ship and per-side ISK efficiency is wrong.
            if root[ai] != root[v] && scored.insert(root[ai]) {
                let s = sides.get_mut(&root[ai]).unwrap();
                s.kills += 1;
                s.isk_destroyed += e.isk;
            }
        }
    }

    let mut out: Vec<Side> = sides
        .into_values()
        .map(|agg| {
            let mut members: Vec<usize> = agg.members.into_iter().collect();
            members.sort_by_key(|i| std::cmp::Reverse(agg.involve.get(i).copied().unwrap_or(0)));
            let parties: Vec<Party> = members.iter().map(|&i| party_by_key[&keys[i]].clone()).collect();
            // Coalition: the most common one among the side's alliance members.
            let mut votes: HashMap<&str, u32> = HashMap::new();
            for p in &parties {
                if matches!(p.kind, PartyKind::Alliance) {
                    if let Some(c) = crate::packs::coalition_of(p.id) {
                        *votes.entry(c).or_default() += 1;
                    }
                }
            }
            let coalition = votes.into_iter().max_by_key(|(_, c)| *c).map(|(c, _)| c.to_owned());
            Side {
                parties,
                coalition,
                kills: agg.kills,
                losses: agg.losses,
                isk_lost: agg.isk_lost,
                isk_destroyed: agg.isk_destroyed,
            }
        })
        .collect();
    // Largest side first, with a deterministic tiebreak (smallest member id/name) so equal-sized
    // sides keep a fixed order across re-clusters instead of swapping with HashMap iteration.
    let tiebreak = |s: &Side| {
        s.parties.iter().map(|p| (p.id, p.name.clone())).min().unwrap_or((0, String::new()))
    };
    out.sort_by(|a, b| {
        (b.kills + b.losses).cmp(&(a.kills + a.losses)).then_with(|| tiebreak(a).cmp(&tiebreak(b)))
    });
    out
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let r = self.find(self.parent[x]);
            self.parent[x] = r;
        }
        self.parent[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn party(id: i64, name: &str) -> Party {
        Party {
            id,
            name: name.to_owned(),
            kind: PartyKind::Alliance,
        }
    }

    // A distinct party id per name, so id-keyed side inference treats them separately.
    fn pid(name: &str) -> i64 {
        name.bytes().map(|b| b as i64).sum::<i64>() + name.len() as i64 * 1000
    }

    fn atk(p: Party) -> Attacker {
        Attacker { char_id: p.id, pilot: p.name.clone(), party: p, ship: 0, final_blow: false }
    }

    fn eng(kill: i64, time: i64, sys: i64, victim: &str, attacker: &str) -> Engagement {
        Engagement {
            kill_id: kill,
            time,
            system_id: sys,
            system_name: format!("S{sys}"),
            security: 0.0,
            victim: party(pid(victim), victim),
            victim_char: pid(victim),
            victim_pilot: victim.to_owned(),
            victim_ship: 587,
            attackers: vec![atk(party(pid(attacker), attacker))],
            isk: 1.0,
            anchored: true,
        }
    }

    // One kill with several attacker parties (each pilot's char_id == its party id, like atk()).
    fn eng_multi(kill: i64, time: i64, sys: i64, victim: &str, attackers: &[&str]) -> Engagement {
        Engagement {
            attackers: attackers.iter().map(|n| atk(party(pid(n), n))).collect(),
            ..eng(kill, time, sys, victim, attackers[0])
        }
    }

    // An attacker in `alliance` flown by a specific character id (distinct from the alliance id),
    // for staging tests where the same alliance fields different pilots across a lull.
    fn atk_char(alliance: &str, char_id: i64) -> Attacker {
        Attacker {
            char_id,
            pilot: format!("c{char_id}"),
            party: party(pid(alliance), alliance),
            ship: 0,
            final_blow: false,
        }
    }

    // A kill with an explicit attacker list.
    fn eng_av(kill: i64, time: i64, sys: i64, victim: &str, attackers: Vec<Attacker>) -> Engagement {
        Engagement { attackers, ..eng(kill, time, sys, victim, victim) }
    }

    // Distance over a line: 1 - 2 - 3 - 4 - 5, plus a bridge 1 <-> 5.
    fn dist(a: i64, b: i64) -> Option<u32> {
        let gate = (a - b).unsigned_abs() as u32;
        let bridge = if (a == 1 && b == 5) || (a == 5 && b == 1) {
            Some(1)
        } else {
            None
        };
        [Some(gate), bridge].into_iter().flatten().min()
    }

    #[test]
    fn chains_within_time_and_jumps() {
        // A@sys1 t=0, B@sys2 t=200 (1 jump, < break_gap) -> same battle (chained).
        // C@sys4 t=1000 is >10 min after both -> separate battle.
        let engs = [
            eng(1, 0, 1, "Red", "Blue"),
            eng(2, 200, 2, "Blue", "Red"),
            eng(3, 1000, 4, "Green", "Blue"),
        ];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 2);
        let big = battles.iter().max_by_key(|b| b.kills).unwrap();
        assert_eq!(big.kills, 2);
        assert_eq!(big.systems.len(), 2);
    }

    #[test]
    fn anchored_battle_keeps_unanchored_kills_whole() {
        // The Kronos scenario: an out-of-range / pre-report kill (anchored=false) that shares a
        // belligerent with an anchored kill in the same window clusters into ONE battle, and that
        // battle counts as anchored — so the whole fight (including the unanchored kill) is kept.
        let unanchored = Engagement { anchored: false, ..eng(1, 0, 1, "Kronos", "Blue") };
        let anchored = eng(2, 240, 1, "Rorqual", "Blue"); // 4 min later, same system, shares Blue
        let battles = cluster(&[unanchored, anchored], BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 1, "should be one battle");
        assert_eq!(battles[0].kills, 2, "both kills present");
        assert!(battles[0].is_anchored(), "battle touches the anchor, so it's kept");
    }

    #[test]
    fn fully_unanchored_battle_is_not_surfaced() {
        // A fight entirely outside the watched area (every kill anchored=false) shares no
        // belligerent with anything anchored -> its cluster is not anchored and is dropped.
        let e1 = Engagement { anchored: false, ..eng(1, 0, 2, "Red", "Blue") };
        let e2 = Engagement { anchored: false, ..eng(2, 60, 2, "Blue", "Red") };
        let battles = cluster(&[e1, e2], BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 1);
        assert!(!battles[0].is_anchored(), "fully out-of-area battle must be filtered out");
        // The post-cluster filter the app applies:
        assert_eq!(battles.into_iter().filter(|b| b.is_anchored()).count(), 0);
    }

    #[test]
    fn unrelated_fights_do_not_merge() {
        // Same system, same time, but disjoint belligerents — two separate battles.
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 30, 1, "Green", "Yellow")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 2, "unrelated fights merged into one BR");
    }

    #[test]
    fn shared_party_chains_across_systems() {
        // Fights in different systems sharing Blue chain into one battle.
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 2, "Green", "Blue")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 1);
        assert_eq!(battles[0].kills, 2);
    }

    #[test]
    fn jump_bridge_links_distant_systems() {
        // sys1 and sys5 are 4 gates apart but 1 bridge jump -> same battle.
        let engs = [
            eng(1, 0, 1, "Red", "Blue"),
            eng(2, 120, 5, "Blue", "Red"),
        ];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 1);
        assert_eq!(battles[0].kills, 2);
    }

    #[test]
    fn cluster_cached_matches_cluster_and_reuses() {
        let engs = vec![
            eng(1, 0, 1, "Red", "Blue"),
            eng(2, 120, 1, "Red", "Blue"),
            eng(3, 5000, 3, "Green", "Gold"), // separate battle (far in time)
        ];
        let plain = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        let mut cache = std::collections::HashMap::new();
        let first = cluster_cached(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist, &mut cache);
        // Same partition (kills + side counts) as the uncached clustering, and the cache holds
        // one entry per battle so the next pass can reuse the unchanged ones.
        let sig = |bs: &[Battle]| {
            let mut v: Vec<(usize, usize)> = bs.iter().map(|b| (b.kills, b.sides.len())).collect();
            v.sort_unstable();
            v
        };
        assert_eq!(sig(&first), sig(&plain));
        assert_eq!(cache.len(), plain.len());
        // A second pass over the SAME engagements yields the same result and keeps the cache stable.
        let second = cluster_cached(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist, &mut cache);
        assert_eq!(sig(&second), sig(&plain));
        assert_eq!(cache.len(), plain.len());
    }

    #[test]
    fn one_sided_battle_is_discarded() {
        // Friendly fire (a party killing its own) infers to a single side — not a real fight.
        let ff = build_battle(
            vec![eng(1, 0, 1, "Blue", "Blue"), eng(2, 60, 1, "Blue", "Blue")],
            BATTLE_BREAK_SECS,
        );
        assert!(!ff.is_two_sided(), "friendly fire should be one-sided: {:?}", ff.sides.len());
        // A genuine fight has two sides.
        let real = build_battle(
            vec![eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 1, "Blue", "Red")],
            BATTLE_BREAK_SECS,
        );
        assert!(real.is_two_sided());
    }

    #[test]
    fn real_3usx_fight_infers_two_sides() {
        // A real >50B-ISK fight in 3USX-F (411 killmails). The previous union-find inference
        // fragmented it into FIVE sides (two coalitions plus stranded stragglers); net-alliance
        // clustering recovers the two belligerent coalitions. Fixture: one line per kill — first id
        // is the victim party, the rest are attacker parties (real alliance/corp ids).
        let data = include_str!("battle_3usx_fight.txt");
        let engs: Vec<Engagement> = data
            .lines()
            .enumerate()
            .filter_map(|(i, line)| {
                let ids: Vec<i64> = line.split_whitespace().filter_map(|s| s.parse().ok()).collect();
                let (&victim, attackers) = ids.split_first()?;
                Some(Engagement {
                    victim: party(victim, &victim.to_string()),
                    victim_char: victim,
                    attackers: attackers.iter().map(|&id| atk(party(id, &id.to_string()))).collect(),
                    ..eng(i as i64, i as i64, 1, "v", "v")
                })
            })
            .collect();
        let sides = infer_sides(&engs);
        let sizes: Vec<usize> = sides.iter().map(|s| s.parties.len()).collect();
        assert_eq!(sides.len(), 2, "expected two coalitions, got sizes {sizes:?}");
        assert!(sizes.iter().all(|&n| n >= 3), "a side is a lone straggler: {sizes:?}");
    }

    #[test]
    fn opposing_coalitions_not_bridged_by_a_killsteal() {
        // Two coalitions, each two alliances, with overwhelming mutual hostility (A1↔B1, A2↔B2).
        // A single cross-side kill-steal (A1 and B2 both land on a neutral Z) must NOT bridge the
        // coalitions into one side — they fought each other far too much.
        let multi = |k: i64, victim: &str, attackers: &[&str]| Engagement {
            attackers: attackers.iter().map(|n| atk(party(pid(n), n))).collect(),
            ..eng(k, k, 1, victim, attackers[0])
        };
        // Distinct names (the test pid() hashes by byte sum, so e.g. "A2"/"B1" would collide).
        // Side A = Alpha+Bravo, Side B = Foxtrot+Golf. Bravo and Golf never fight each other —
        // they only co-attack a neutral Zulu once (the kill-steal that previously bridged sides).
        let engs = vec![
            multi(1, "Foxtrot", &["Alpha", "Bravo"]), // A hits B's Foxtrot
            multi(2, "Foxtrot", &["Alpha", "Bravo"]),
            multi(3, "Alpha", &["Foxtrot", "Golf"]), // B hits A's Alpha
            multi(4, "Alpha", &["Foxtrot", "Golf"]),
            multi(5, "Zulu", &["Bravo", "Golf"]), // kill-steal: cross-side, no direct hostility
        ];
        let sides = infer_sides(&engs);
        let side_of =
            |name: &str| sides.iter().position(|s| s.parties.iter().any(|p| p.name == name));
        // Alpha/Bravo vs Foxtrot/Golf are overwhelmingly hostile — never one side.
        assert_ne!(side_of("Alpha"), side_of("Foxtrot"), "opposing coalitions bridged into one side");
        assert_eq!(side_of("Alpha"), side_of("Bravo"), "A coalition split apart");
        assert_eq!(side_of("Foxtrot"), side_of("Golf"), "B coalition split apart");
    }

    #[test]
    fn sides_split_by_kills_and_losses() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 1, "Red", "Blue")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        let b = &battles[0];
        assert_eq!(b.sides.len(), 2);
        let has = |s: &&Side, name: &str| s.parties.iter().any(|p| p.name == name);
        let blue = b.sides.iter().find(|s| has(s, "Blue")).unwrap();
        let red = b.sides.iter().find(|s| has(s, "Red")).unwrap();
        assert_eq!((blue.kills, blue.losses), (2, 0));
        assert_eq!((red.kills, red.losses), (0, 2));
    }

    #[test]
    fn coattackers_form_one_side() {
        // Blue + Green kill Red together -> one side {Blue, Green} vs {Red}.
        let mut e = eng(1, 0, 1, "Red", "Blue");
        e.attackers.push(atk(party(3, "Green")));
        let battles = cluster(std::slice::from_ref(&e), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        let b = &battles[0];
        assert_eq!(b.sides.len(), 2);
        let allied = b.sides.iter().find(|s| s.parties.iter().any(|p| p.name == "Blue")).unwrap();
        assert!(allied.parties.iter().any(|p| p.name == "Green"));
        assert_eq!(allied.kills, 1);
    }

    #[test]
    fn shared_target_merges_alliance_less_groups() {
        // Two pilots in different NPC corps (no alliance) each kill the same enemy once, on
        // separate mails — never shooting each other. They are one side, not two.
        let engs = [eng(1, 0, 1, "Foe", "Aay"), eng(2, 60, 1, "Foe", "Bee")];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        assert_eq!(b.sides.len(), 2);
        let aay = b.side_of(&party(pid("Aay"), "Aay")).unwrap();
        let bee = b.side_of(&party(pid("Bee"), "Bee")).unwrap();
        assert_eq!(aay, bee, "alliance-less attackers split: {:?}", b.sides);
    }

    #[test]
    fn friendly_fire_does_not_split_a_side() {
        // Aay + Bee shoot Cee together (allies); Aay accidentally kills Bee once (< SIGNIF).
        let mut together = eng(1, 0, 1, "Cee", "Aay");
        together.attackers.push(atk(party(pid("Bee"), "Bee")));
        let engs = [
            together,
            eng(2, 10, 1, "Cee", "Aay"),
            eng(3, 20, 1, "Cee", "Bee"),
            eng(4, 30, 1, "Bee", "Aay"), // single friendly-fire kill
        ];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let a = b.sides.iter().position(|s| s.parties.iter().any(|p| p.name == "Aay")).unwrap();
        let bee = b.sides.iter().position(|s| s.parties.iter().any(|p| p.name == "Bee")).unwrap();
        assert_eq!(a, bee, "friendly fire split the side: {:?}", b.sides);
    }

    #[test]
    fn isk_efficiency_per_side() {
        // Blue kills two 10-ISK Red ships; Red kills one 10-ISK Blue ship.
        let val = |kill, time, victim, attacker, isk| Engagement { isk, ..eng(kill, time, 1, victim, attacker) };
        let engs = [
            val(1, 0, "Red", "Blue", 10.0),
            val(2, 60, "Red", "Blue", 10.0),
            val(3, 120, "Blue", "Red", 10.0),
        ];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let blue = b.sides.iter().find(|s| s.parties.iter().any(|p| p.name == "Blue")).unwrap();
        let red = b.sides.iter().find(|s| s.parties.iter().any(|p| p.name == "Red")).unwrap();
        // Blue: destroyed 20, lost 10 -> 66.7%. Red: destroyed 10, lost 20 -> 33.3%.
        assert!((blue.isk_efficiency().unwrap() - 200.0 / 3.0).abs() < 1e-6);
        assert!((red.isk_efficiency().unwrap() - 100.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn involvement_tracks_kills() {
        // Blue + Green kill Red on one mail.
        let mut k1 = eng(1, 0, 1, "Red", "Blue");
        k1.attackers.push(atk(party(pid("Green"), "Green")));
        let b = &cluster(std::slice::from_ref(&k1), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let inv = b.involvement();
        // Both helped kill Red.
        assert!(inv.killed[&pid("Blue")].contains(&pid("Red")));
        assert!(inv.killed[&pid("Green")].contains(&pid("Red")));
        // Red's killers (for the border) are Blue and Green.
        assert!(inv.attackers[&1].contains(&pid("Blue")) && inv.attackers[&1].contains(&pid("Green")));
    }

    #[test]
    fn pods_fold_into_ship_and_survivors_dedup() {
        let loss_eng = |kill, time, victim: &str, ship, attacker: &str, isk| Engagement {
            victim_ship: ship,
            isk,
            ..eng(kill, time, 1, victim, attacker)
        };
        // Blue + Green kill Red's ship then Red's pod. Neither Blue nor Green dies.
        let mut k1 = loss_eng(1, 0, "Red", 24692, "Blue", 100.0);
        k1.attackers.push(atk(party(pid("Green"), "Green")));
        let engs = [k1, loss_eng(2, 5, "Red", 670, "Blue", 1.0)];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];

        let red = b.side_of(&party(pid("Red"), "Red")).unwrap();
        let red_roster = b.roster(red);
        let red_lost: Vec<_> = red_roster.iter().filter(|p| p.lost.is_some()).collect();
        // The pod folded into the ship row: one loss, hull value 100 + pod value 1 tracked apart.
        assert_eq!(red_lost.len(), 1);
        assert_eq!(red_lost[0].ship, 24692);
        let lost = red_lost[0].lost.as_ref().unwrap();
        assert!((lost.value - 100.0).abs() < 1e-6);
        assert!((lost.pod_value - 1.0).abs() < 1e-6);

        let blue = b.side_of(&party(pid("Blue"), "Blue")).unwrap();
        let survivors: Vec<_> = b.roster(blue).into_iter().filter(|p| p.lost.is_none()).collect();
        // Blue and Green each survive, listed once despite appearing on two kills.
        assert_eq!(survivors.len(), 2);
        assert_eq!(survivors.iter().filter(|s| s.pilot == "Green").count(), 1);
    }

    #[test]
    fn roster_orders_by_value_descending() {
        // Red loses a pricey Abaddon (24692, 100 ISK) and a cheap Rifter (587, 5 ISK). The roster
        // lists the highest-value kill first.
        let v = |kill, time, victim: &str, ship, attacker: &str, isk| Engagement {
            victim_ship: ship,
            isk,
            ..eng(kill, time, 1, victim, attacker)
        };
        let engs = [v(1, 0, "Red", 587, "Blue", 5.0), v(2, 30, "Red", 24692, "Blue", 100.0)];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let red = b.side_of(&party(pid("Red"), "Red")).unwrap();
        let roster = b.roster(red);
        // Most valuable loss (Abaddon, 100) before the cheap one (Rifter, 5).
        let ships: Vec<i64> = roster.iter().map(|p| p.ship).collect();
        assert_eq!(ships, vec![24692, 587]);
        assert!(roster.iter().all(|p| p.lost.is_some()));
    }

    #[test]
    fn matches_filters_by_system_and_pilot() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 1, "Red", "Blue")];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        assert!(b.matches(""));            // empty query matches all
        assert!(b.matches("s1"));          // system name (eng() names systems "S{id}")
        assert!(b.matches("red"));         // victim party / pilot, case-insensitive
        assert!(b.matches("blue"));        // attacker
        assert!(!b.matches("goonswarm")); // unrelated term
    }

    #[test]
    fn friendly_fire_not_credited_as_destroyed() {
        // Blue kills Red (100), Red kills Blue (100), then a Blue dies to a Blue (40, friendly).
        // Blue must not be credited with destroying that 40 ISK — it shot its own ship.
        let val = |kill, time, victim, attacker, isk| Engagement {
            isk,
            ..eng(kill, time, 1, victim, attacker)
        };
        let engs = [
            val(1, 0, "Red", "Blue", 100.0),
            val(2, 30, "Blue", "Red", 100.0),
            val(3, 60, "Blue", "Blue", 40.0),
        ];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let blue = b.sides.iter().find(|s| s.parties.iter().any(|p| p.name == "Blue")).unwrap();
        assert!((blue.isk_destroyed - 100.0).abs() < 1e-6, "destroyed={}", blue.isk_destroyed);
        assert!((blue.isk_lost - 140.0).abs() < 1e-6, "lost={}", blue.isk_lost);
        // Self-kill isn't counted as a kill for Blue.
        assert_eq!(blue.kills, 1);
    }

    #[test]
    fn side_groups_by_coalition() {
        // Two Imperium alliances co-attack an enemy → one side labelled "The Imperium".
        let goon = party(1354830081, "Goonswarm Federation");
        let imp2 = party(99010079, "Imp2");
        let enemy = party(99999999, "Enemy");
        let e = Engagement {
            kill_id: 1,
            time: 0,
            system_id: 1,
            system_name: "S1".into(),
            security: 0.0,
            victim_char: 0,
            victim_pilot: enemy.name.clone(),
            victim: enemy,
            victim_ship: 587,
            attackers: vec![atk(goon), atk(imp2)],
            isk: 1.0,
            anchored: true,
        };
        let b = &cluster(std::slice::from_ref(&e), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let imp = b.sides.iter().find(|s| s.parties.iter().any(|p| p.id == 1354830081)).unwrap();
        assert_eq!(imp.coalition.as_deref(), Some("The Imperium"));
        assert!(imp.parties.iter().any(|p| p.id == 99010079));
    }

    fn cluster_def(engs: &[Engagement]) -> Vec<Battle> {
        cluster(engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)
    }

    #[test]
    fn auto_segment_splits_on_long_lull() {
        // A shared-party chain across a lull longer than break_gap is two battles; a shorter lull
        // keeps it as one.
        let long = [eng(1, 0, 1, "Red", "Blue"), eng(2, 400, 1, "Blue", "Red")];
        assert_eq!(cluster_def(&long).len(), 2, "a > break_gap lull should split the battle");

        let short = [eng(1, 0, 1, "Red", "Blue"), eng(2, 200, 1, "Blue", "Red")];
        assert_eq!(cluster_def(&short).len(), 1, "a < break_gap lull stays one battle");
    }

    #[test]
    fn stray_kills_do_not_bridge_dense_segments() {
        // Two dense bursts (3 kills each) joined only by a string of strays, each < break_gap apart
        // (so no hard split) — the density valley separates them into two dense battles.
        let times = [0, 30, 60, 350, 640, 930, 1220, 1250, 1280];
        let engs: Vec<Engagement> =
            times.iter().enumerate().map(|(i, &t)| eng(i as i64 + 1, t, 1, "Red", "Blue")).collect();
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 2, "density valley should split the strays-bridged bursts");
        assert!(battles.iter().all(|b| b.kills >= DENSE_MIN), "a split side is not dense");
    }

    #[test]
    fn single_stray_does_not_split_a_dense_fight() {
        // One trailing stray within break_gap of a dense fight must not split it.
        let times = [0, 30, 60, 90, 120, 150, 400];
        let engs: Vec<Engagement> =
            times.iter().enumerate().map(|(i, &t)| eng(i as i64 + 1, t, 1, "Red", "Blue")).collect();
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 1, "a single stray split a dense fight");
        assert_eq!(battles[0].kills, 7);
    }

    #[test]
    fn chase_merges_across_gap_same_pilots() {
        // Two bursts by the SAME pilots, ~12 min apart and in range — one running fight (a chase).
        let atks = ["P1", "P2", "P3", "P4"];
        let engs = vec![
            eng_multi(1, 0, 1, "VA1", &atks),
            eng_multi(2, 30, 1, "VA2", &atks),
            eng_multi(3, 60, 1, "VA3", &atks),
            eng_multi(4, 780, 1, "VB1", &atks),
            eng_multi(5, 810, 1, "VB2", &atks),
            eng_multi(6, 840, 1, "VB3", &atks),
        ];
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 1, "same pilots across the gap should chase-merge");
        assert_eq!(battles[0].kills, 6);
    }

    #[test]
    fn staging_skirmishes_same_alliance_do_not_chase_merge() {
        // Same alliance, but DIFFERENT pilots on each side of the gap (two separate skirmishes from
        // a staging system) — char-level overlap is zero, so they must not chase-merge.
        let a = |id| atk_char("Ally", id);
        let engs = vec![
            eng_av(1, 0, 1, "VA1", vec![a(1001), a(1002), a(1003)]),
            eng_av(2, 30, 1, "VA2", vec![a(1001), a(1002), a(1003)]),
            eng_av(3, 60, 1, "VA3", vec![a(1001), a(1002), a(1003)]),
            eng_av(4, 780, 1, "VB1", vec![a(2001), a(2002), a(2003)]),
            eng_av(5, 810, 1, "VB2", vec![a(2001), a(2002), a(2003)]),
            eng_av(6, 840, 1, "VB3", vec![a(2001), a(2002), a(2003)]),
        ];
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 2, "distinct pilots of one alliance must not chase-merge");
    }

    #[test]
    fn chase_merge_respects_span_cap() {
        // A long run of bursts by the same pilots, each merge-gap apart: the accumulated chase stops
        // growing once it would exceed MERGE_MAX_SPAN, leaving the final burst as its own battle.
        let atks = ["P1", "P2", "P3"];
        let times = [0, 1100, 2200, 3300, 4400, 5500, 6600, 7700];
        let engs: Vec<Engagement> = times
            .iter()
            .enumerate()
            .map(|(i, &t)| eng_multi(i as i64 + 1, t, 1, &format!("V{i}"), &atks))
            .collect();
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 2, "span cap should cut the over-long chase");
    }

    #[test]
    fn ambiguity_flags_bridge_kill() {
        // Left burst {L} kills, a single bridge kill (L hits R), then a right burst {R} kills. No
        // participant spans across the bridge -> it is flagged as a suggested split point.
        let engs = vec![
            eng(0, 0, 1, "v0", "L"),
            eng(1, 30, 1, "v1", "L"),
            eng(2, 60, 1, "v2", "L"),
            eng(3, 90, 1, "R", "L"),
            eng(4, 120, 1, "v4", "R"),
            eng(5, 150, 1, "v5", "R"),
            eng(6, 180, 1, "v6", "R"),
        ];
        let b = build_battle(engs, BATTLE_BREAK_SECS);
        assert!(b.ambiguous, "bridge kill should make the battle ambiguous");
        assert!(b.suggested_splits.contains(&90), "splits {:?}", b.suggested_splits);
    }

    #[test]
    fn ambiguity_flags_near_threshold_gap() {
        // A lull just under break_gap (240s vs 300) keeps the battle whole but flags it as ambiguous
        // with the lull as a suggested split.
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 240, 1, "Blue", "Red")];
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 1);
        assert!(battles[0].ambiguous, "near-threshold lull should be ambiguous");
        assert!(battles[0].suggested_splits.contains(&240));
    }
}
