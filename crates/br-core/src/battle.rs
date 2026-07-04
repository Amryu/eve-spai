use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const BATTLE_WINDOW_SECS: i64 = 600;
pub const BATTLE_MAX_JUMPS: u32 = 3;
pub const BATTLE_BREAK_SECS: i64 = 300;
const CHASE_OVERLAP: f64 = 0.5;
const MERGE_MAX_GAP: i64 = 1200;
const MERGE_MAX_JUMPS: u32 = 10;
const MERGE_MAX_SPAN: i64 = 7200;
const DENSE_MIN: usize = 3;

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Overrides {
    pub tag: HashMap<i64, i64>,
    pub excluded: HashSet<i64>,
    pub scrubs: HashSet<(i64, i64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BattleReportDoc {
    pub format_version: u32,
    pub generator: String,
    pub exported_at: i64,
    pub title: Option<String>,
    pub battle: Battle,
    pub engagements: Vec<Engagement>,
    pub overrides: Overrides,
    #[serde(default)]
    pub ship_names: std::collections::BTreeMap<i64, String>,
    #[serde(default)]
    pub affiliations: std::collections::BTreeMap<i64, Affil>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Affil {
    pub corp_id: i64,
    pub corp_name: String,
    pub alliance_id: i64,
    pub alliance_name: String,
}

impl BattleReportDoc {
    pub const FORMAT_VERSION: u32 = 1;

    pub fn new(
        battle: Battle,
        engagements: Vec<Engagement>,
        overrides: Overrides,
        title: Option<String>,
        exported_at: i64,
        ship_names: std::collections::BTreeMap<i64, String>,
        affiliations: std::collections::BTreeMap<i64, Affil>,
    ) -> Self {
        Self {
            format_version: Self::FORMAT_VERSION,
            generator: concat!("eve-spai/", env!("CARGO_PKG_VERSION")).to_string(),
            exported_at,
            title,
            battle,
            engagements,
            overrides,
            ship_names,
            affiliations,
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> anyhow::Result<Self> {
        let doc: BattleReportDoc = serde_json::from_str(s)?;
        if doc.format_version != Self::FORMAT_VERSION {
            anyhow::bail!(
                "unsupported battle-report format v{} (this build understands v{}) — update EVE Spai",
                doc.format_version,
                Self::FORMAT_VERSION
            );
        }
        Ok(doc)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartyKind {
    Alliance,
    Corporation,
    Character,
    Faction,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Party {
    #[allow(dead_code)]
    pub id: i64,
    pub name: String,
    #[allow(dead_code)]
    pub kind: PartyKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attacker {
    pub party: Party,
    pub char_id: i64,
    pub ship: i64,
    pub pilot: String,
    pub final_blow: bool,
}

/// Capsule (pod) ship type ids: regular and the Genolution variant. A pod kill is folded
/// into its pilot's ship kill rather than shown separately.
pub const POD_TYPES: [i64; 2] = [670, 33328];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Engagement {
    pub kill_id: i64,
    pub time: i64,
    pub system_id: i64,
    pub system_name: String,
    pub security: f64,
    pub victim: Party,
    pub victim_char: i64,
    pub victim_pilot: String,
    pub victim_ship: i64,
    pub attackers: Vec<Attacker>,
    pub isk: f64,
    #[serde(default = "default_true")]
    pub anchored: bool,
}

fn default_true() -> bool {
    true
}

impl Engagement {
    #[allow(dead_code)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Side {
    pub parties: Vec<Party>,
    pub coalition: Option<String>,
    pub kills: u32,
    pub losses: u32,
    pub isk_lost: f64,
    pub isk_destroyed: f64,
}

impl Side {
    pub fn isk_efficiency(&self) -> Option<f64> {
        let total = self.isk_destroyed + self.isk_lost;
        (total > 0.0).then(|| self.isk_destroyed / total * 100.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Battle {
    pub engagements: Vec<Engagement>,
    pub start: i64,
    pub end: i64,
    pub systems: Vec<(i64, String, f64)>,
    pub sides: Vec<Side>,
    pub kills: usize,
    pub isk: f64,
    pub ambiguous: bool,
    pub suggested_splits: Vec<SplitSuggestion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitReason {
    Lull,
    BridgeKill,
    SubFights,
}

impl SplitReason {
    pub fn label(self) -> &'static str {
        match self {
            SplitReason::Lull => "quiet gap",
            SplitReason::BridgeKill => "single bridging kill",
            SplitReason::SubFights => "distinct sub-fights",
        }
    }
    fn priority(self) -> u8 {
        match self {
            SplitReason::BridgeKill => 2,
            SplitReason::SubFights => 1,
            SplitReason::Lull => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplitSuggestion {
    pub time: i64,
    pub reason: SplitReason,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lost {
    pub kill_id: i64,
    pub value: f64,
    pub pod_value: f64,
    pub pod_ship: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    pub char_id: i64,
    pub party: Party,
    pub pilot: String,
    pub ship: i64,
    pub lost: Option<Lost>,
}

#[derive(Default)]
pub struct Involvement {
    pub killed: HashMap<i64, std::collections::HashSet<i64>>,
    pub attackers: HashMap<i64, std::collections::HashSet<i64>>,
}

impl Battle {
    pub fn is_anchored(&self) -> bool {
        self.engagements.iter().any(|e| e.anchored)
    }

    pub fn is_two_sided(&self) -> bool {
        self.sides.len() >= 2
    }

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
    pub fn side_of(&self, p: &Party) -> Option<usize> {
        self.sides.iter().position(|s| {
            s.parties.iter().any(|q| if p.id != 0 { q.id == p.id } else { q.name == p.name })
        })
    }

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
            match ships.iter_mut().find(|s| same(s, &pod)) {
                Some(ship) => {
                    ship.pod_value += pod.value;
                    ship.pod_ship = pod.ship;
                }
                None => ships.push(pod),
            }
        }

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
                    continue;
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
    battles.sort_by(|a, b| b.end.cmp(&a.end));
    battles
}

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
    battles.sort_by(|a, b| b.end.cmp(&a.end));
    battles
}

fn partition_battles(
    engagements: &[Engagement],
    window: i64,
    max_jumps: u32,
    break_gap: i64,
    overrides: &Overrides,
    dist: &impl Fn(i64, i64) -> Option<u32>,
) -> (Vec<Engagement>, Vec<Vec<usize>>) {
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

    let groups = group_indices(&filtered, window, max_jumps, &overrides.tag, dist);

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

    let mut final_groups = merge_chases(&filtered, untagged_segments, dist);
    final_groups.extend(tagged_groups);
    (filtered, final_groups)
}

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
    let mut root_tag: HashMap<usize, i64> = HashMap::new();
    for (t, members) in &by_tag {
        root_tag.insert(uf.find(members[0]), *t);
    }

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| engagements[i].time);
    for oi in 0..n {
        let i = order[oi];
        for &j in order.iter().skip(oi + 1) {
            if engagements[j].time - engagements[i].time >= window {
                break;
            }
            let (ri, rj) = (uf.find(i), uf.find(j));
            if parties[i].is_disjoint(&parties[j]) || ri == rj {
                continue;
            }
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

fn time_bounds(engs: &[Engagement], idxs: &[usize]) -> (i64, i64) {
    let mut lo = i64::MAX;
    let mut hi = i64::MIN;
    for &i in idxs {
        lo = lo.min(engs[i].time);
        hi = hi.max(engs[i].time);
    }
    (lo, hi)
}

fn segment_indices(engs: &[Engagement], mut idxs: Vec<usize>, break_gap: i64) -> Vec<Vec<usize>> {
    idxs.sort_by_key(|&i| engs[i].time);
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
    let mut out: Vec<Vec<usize>> = Vec::new();
    for seg in hard {
        refine_density(engs, seg, break_gap, &mut out);
    }
    out
}

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

fn valley_split(engs: &[Engagement], seg: &[usize], break_gap: i64) -> Option<usize> {
    let start = engs[seg[0]].time;
    let to_bin = |t: i64| ((t - start) / break_gap) as usize;
    let nbins = to_bin(engs[*seg.last().unwrap()].time) + 1;
    if nbins < 3 {
        return None;
    }
    let mut counts = vec![0usize; nbins];
    for &i in seg {
        counts[to_bin(engs[i].time)] += 1;
    }
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
        let hi = b - 1;
        if lo == 0 || hi == nbins - 1 {
            continue;
        }
        let mut core_l = 0usize;
        let mut p = lo;
        while p > 0 && counts[p - 1] >= 2 {
            p -= 1;
            core_l += counts[p];
        }
        let mut core_r = 0usize;
        let mut q = hi + 1;
        while q < nbins && counts[q] >= 2 {
            core_r += counts[q];
            q += 1;
        }
        if core_l < DENSE_MIN || core_r < DENSE_MIN {
            continue;
        }
        let strays: Vec<usize> = seg
            .iter()
            .copied()
            .filter(|&i| (lo..=hi).contains(&to_bin(engs[i].time)))
            .collect();
        let (sl, sh) = time_bounds(engs, &strays);
        if sh - sl < break_gap {
            continue;
        }
        let cut_bin = lo + (hi - lo + 1) / 2;
        let k = seg.iter().position(|&i| to_bin(engs[i].time) >= cut_bin)?;
        if k == 0 || k == seg.len() {
            continue;
        }
        return Some(k);
    }
    None
}

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

fn battle_ambiguity(engs: &[Engagement], break_gap: i64) -> (bool, Vec<SplitSuggestion>) {
    let n = engs.len();
    let mut splits: Vec<SplitSuggestion> = Vec::new();

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
            splits.push(SplitSuggestion { time: engs[at].time, reason: SplitReason::Lull });
        }
    }

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
                splits.push(SplitSuggestion { time: engs[k].time, reason: SplitReason::BridgeKill });
            }
        }
    }

    {
        let subs = segment_indices(engs, (0..n).collect(), break_gap / 2);
        if subs.iter().filter(|s| s.len() >= DENSE_MIN).count() >= 2 {
            let mut subs = subs;
            subs.sort_by_key(|s| time_bounds(engs, s).0);
            for w in subs.windows(2) {
                if w[0].len() >= DENSE_MIN && w[1].len() >= DENSE_MIN {
                    splits.push(SplitSuggestion {
                        time: time_bounds(engs, &w[1]).0,
                        reason: SplitReason::SubFights,
                    });
                }
            }
        }
    }

    splits.sort_by(|a, b| {
        a.time.cmp(&b.time).then(b.reason.priority().cmp(&a.reason.priority()))
    });
    splits.dedup_by_key(|s| s.time);
    splits.truncate(3);
    (!splits.is_empty(), splits)
}

fn infer_sides(engs: &[Engagement]) -> Vec<Side> {
    let key = |p: &Party| if p.id != 0 { format!("#{}", p.id) } else { format!("n:{}", p.name) };
    let mut party_by_key: HashMap<String, Party> = HashMap::new();
    for e in engs {
        party_by_key.entry(key(&e.victim)).or_insert_with(|| e.victim.clone());
        for a in &e.attackers {
            party_by_key.entry(key(&a.party)).or_insert_with(|| a.party.clone());
        }
    }
    // Sort the keys so party→index assignment is deterministic. HashMap key order is randomized
    // per map instance, which made the agglomerative merge's net-score tie-breaks (lowest index
    // wins) — and thus the side partition — differ on every call, so the split preview flickered.
    let mut keys: Vec<String> = party_by_key.keys().cloned().collect();
    keys.sort_unstable();
    let idx: HashMap<String, usize> =
        keys.iter().cloned().enumerate().map(|(i, k)| (k, i)).collect();
    let n = keys.len();

    let mut hostility: HashMap<(usize, usize), u32> = HashMap::new();
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
        let mut best_net = -1i64;
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

    fn eng_multi(kill: i64, time: i64, sys: i64, victim: &str, attackers: &[&str]) -> Engagement {
        Engagement {
            attackers: attackers.iter().map(|n| atk(party(pid(n), n))).collect(),
            ..eng(kill, time, sys, victim, attackers[0])
        }
    }

    fn atk_char(alliance: &str, char_id: i64) -> Attacker {
        Attacker {
            char_id,
            pilot: format!("c{char_id}"),
            party: party(pid(alliance), alliance),
            ship: 0,
            final_blow: false,
        }
    }

    fn eng_av(kill: i64, time: i64, sys: i64, victim: &str, attackers: Vec<Attacker>) -> Engagement {
        Engagement { attackers, ..eng(kill, time, sys, victim, victim) }
    }

    #[test]
    fn infer_sides_is_deterministic() {
        let engs = vec![
            eng_multi(1, 0, 1, "Red", &["Blue", "Green"]),
            eng_multi(2, 10, 1, "Blue", &["Red", "Yellow"]),
            eng_multi(3, 20, 1, "Green", &["Blue", "Yellow"]),
            eng_multi(4, 30, 1, "Yellow", &["Red", "Green"]),
            eng_multi(5, 40, 1, "Red", &["Blue", "Green", "Yellow"]),
            eng_multi(6, 50, 1, "Blue", &["Red"]),
        ];
        let fingerprint = |b: &Battle| -> Vec<Vec<i64>> {
            b.sides.iter().map(|s| s.parties.iter().map(|p| p.id).collect()).collect()
        };
        let first = fingerprint(&preview_battle(engs.clone(), BATTLE_BREAK_SECS));
        for _ in 0..16 {
            assert_eq!(
                fingerprint(&preview_battle(engs.clone(), BATTLE_BREAK_SECS)),
                first,
                "infer_sides is non-deterministic — sides would flicker"
            );
        }
    }

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
        let unanchored = Engagement { anchored: false, ..eng(1, 0, 1, "Kronos", "Blue") };
        let anchored = eng(2, 240, 1, "Rorqual", "Blue");
        let battles = cluster(&[unanchored, anchored], BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 1, "should be one battle");
        assert_eq!(battles[0].kills, 2, "both kills present");
        assert!(battles[0].is_anchored(), "battle touches the anchor, so it's kept");
    }

    #[test]
    fn fully_unanchored_battle_is_not_surfaced() {
        let e1 = Engagement { anchored: false, ..eng(1, 0, 2, "Red", "Blue") };
        let e2 = Engagement { anchored: false, ..eng(2, 60, 2, "Blue", "Red") };
        let battles = cluster(&[e1, e2], BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 1);
        assert!(!battles[0].is_anchored(), "fully out-of-area battle must be filtered out");
        assert_eq!(battles.into_iter().filter(|b| b.is_anchored()).count(), 0);
    }

    #[test]
    fn unrelated_fights_do_not_merge() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 30, 1, "Green", "Yellow")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 2, "unrelated fights merged into one BR");
    }

    #[test]
    fn shared_party_chains_across_systems() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 2, "Green", "Blue")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        assert_eq!(battles.len(), 1);
        assert_eq!(battles[0].kills, 2);
    }

    #[test]
    fn jump_bridge_links_distant_systems() {
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
            eng(3, 5000, 3, "Green", "Gold"),
        ];
        let plain = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist);
        let mut cache = std::collections::HashMap::new();
        let first = cluster_cached(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist, &mut cache);
        let sig = |bs: &[Battle]| {
            let mut v: Vec<(usize, usize)> = bs.iter().map(|b| (b.kills, b.sides.len())).collect();
            v.sort_unstable();
            v
        };
        assert_eq!(sig(&first), sig(&plain));
        assert_eq!(cache.len(), plain.len());
        let second = cluster_cached(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist, &mut cache);
        assert_eq!(sig(&second), sig(&plain));
        assert_eq!(cache.len(), plain.len());
    }

    #[test]
    fn one_sided_battle_is_discarded() {
        let ff = build_battle(
            vec![eng(1, 0, 1, "Blue", "Blue"), eng(2, 60, 1, "Blue", "Blue")],
            BATTLE_BREAK_SECS,
        );
        assert!(!ff.is_two_sided(), "friendly fire should be one-sided: {:?}", ff.sides.len());
        let real = build_battle(
            vec![eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 1, "Blue", "Red")],
            BATTLE_BREAK_SECS,
        );
        assert!(real.is_two_sided());
    }

    #[test]
    fn real_3usx_fight_infers_two_sides() {
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
        let multi = |k: i64, victim: &str, attackers: &[&str]| Engagement {
            attackers: attackers.iter().map(|n| atk(party(pid(n), n))).collect(),
            ..eng(k, k, 1, victim, attackers[0])
        };
        let engs = vec![
            multi(1, "Foxtrot", &["Alpha", "Bravo"]),
            multi(2, "Foxtrot", &["Alpha", "Bravo"]),
            multi(3, "Alpha", &["Foxtrot", "Golf"]),
            multi(4, "Alpha", &["Foxtrot", "Golf"]),
            multi(5, "Zulu", &["Bravo", "Golf"]),
        ];
        let sides = infer_sides(&engs);
        let side_of =
            |name: &str| sides.iter().position(|s| s.parties.iter().any(|p| p.name == name));
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
        let engs = [eng(1, 0, 1, "Foe", "Aay"), eng(2, 60, 1, "Foe", "Bee")];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        assert_eq!(b.sides.len(), 2);
        let aay = b.side_of(&party(pid("Aay"), "Aay")).unwrap();
        let bee = b.side_of(&party(pid("Bee"), "Bee")).unwrap();
        assert_eq!(aay, bee, "alliance-less attackers split: {:?}", b.sides);
    }

    #[test]
    fn friendly_fire_does_not_split_a_side() {
        let mut together = eng(1, 0, 1, "Cee", "Aay");
        together.attackers.push(atk(party(pid("Bee"), "Bee")));
        let engs = [
            together,
            eng(2, 10, 1, "Cee", "Aay"),
            eng(3, 20, 1, "Cee", "Bee"),
            eng(4, 30, 1, "Bee", "Aay"),
        ];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let a = b.sides.iter().position(|s| s.parties.iter().any(|p| p.name == "Aay")).unwrap();
        let bee = b.sides.iter().position(|s| s.parties.iter().any(|p| p.name == "Bee")).unwrap();
        assert_eq!(a, bee, "friendly fire split the side: {:?}", b.sides);
    }

    #[test]
    fn isk_efficiency_per_side() {
        let val = |kill, time, victim, attacker, isk| Engagement { isk, ..eng(kill, time, 1, victim, attacker) };
        let engs = [
            val(1, 0, "Red", "Blue", 10.0),
            val(2, 60, "Red", "Blue", 10.0),
            val(3, 120, "Blue", "Red", 10.0),
        ];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let blue = b.sides.iter().find(|s| s.parties.iter().any(|p| p.name == "Blue")).unwrap();
        let red = b.sides.iter().find(|s| s.parties.iter().any(|p| p.name == "Red")).unwrap();
        assert!((blue.isk_efficiency().unwrap() - 200.0 / 3.0).abs() < 1e-6);
        assert!((red.isk_efficiency().unwrap() - 100.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn involvement_tracks_kills() {
        let mut k1 = eng(1, 0, 1, "Red", "Blue");
        k1.attackers.push(atk(party(pid("Green"), "Green")));
        let b = &cluster(std::slice::from_ref(&k1), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let inv = b.involvement();
        assert!(inv.killed[&pid("Blue")].contains(&pid("Red")));
        assert!(inv.killed[&pid("Green")].contains(&pid("Red")));
        assert!(inv.attackers[&1].contains(&pid("Blue")) && inv.attackers[&1].contains(&pid("Green")));
    }

    #[test]
    fn pods_fold_into_ship_and_survivors_dedup() {
        let loss_eng = |kill, time, victim: &str, ship, attacker: &str, isk| Engagement {
            victim_ship: ship,
            isk,
            ..eng(kill, time, 1, victim, attacker)
        };
        let mut k1 = loss_eng(1, 0, "Red", 24692, "Blue", 100.0);
        k1.attackers.push(atk(party(pid("Green"), "Green")));
        let engs = [k1, loss_eng(2, 5, "Red", 670, "Blue", 1.0)];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];

        let red = b.side_of(&party(pid("Red"), "Red")).unwrap();
        let red_roster = b.roster(red);
        let red_lost: Vec<_> = red_roster.iter().filter(|p| p.lost.is_some()).collect();
        assert_eq!(red_lost.len(), 1);
        assert_eq!(red_lost[0].ship, 24692);
        let lost = red_lost[0].lost.as_ref().unwrap();
        assert!((lost.value - 100.0).abs() < 1e-6);
        assert!((lost.pod_value - 1.0).abs() < 1e-6);

        let blue = b.side_of(&party(pid("Blue"), "Blue")).unwrap();
        let survivors: Vec<_> = b.roster(blue).into_iter().filter(|p| p.lost.is_none()).collect();
        assert_eq!(survivors.len(), 2);
        assert_eq!(survivors.iter().filter(|s| s.pilot == "Green").count(), 1);
    }

    #[test]
    fn roster_orders_by_value_descending() {
        let v = |kill, time, victim: &str, ship, attacker: &str, isk| Engagement {
            victim_ship: ship,
            isk,
            ..eng(kill, time, 1, victim, attacker)
        };
        let engs = [v(1, 0, "Red", 587, "Blue", 5.0), v(2, 30, "Red", 24692, "Blue", 100.0)];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        let red = b.side_of(&party(pid("Red"), "Red")).unwrap();
        let roster = b.roster(red);
        let ships: Vec<i64> = roster.iter().map(|p| p.ship).collect();
        assert_eq!(ships, vec![24692, 587]);
        assert!(roster.iter().all(|p| p.lost.is_some()));
    }

    #[test]
    fn matches_filters_by_system_and_pilot() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 1, "Red", "Blue")];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, BATTLE_BREAK_SECS, &Overrides::default(), dist)[0];
        assert!(b.matches(""));
        assert!(b.matches("s1"));
        assert!(b.matches("red"));
        assert!(b.matches("blue"));
        assert!(!b.matches("goonswarm"));
    }

    #[test]
    fn friendly_fire_not_credited_as_destroyed() {
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
        assert_eq!(blue.kills, 1);
    }

    #[test]
    fn side_groups_by_coalition() {
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
        let long = [eng(1, 0, 1, "Red", "Blue"), eng(2, 400, 1, "Blue", "Red")];
        assert_eq!(cluster_def(&long).len(), 2, "a > break_gap lull should split the battle");

        let short = [eng(1, 0, 1, "Red", "Blue"), eng(2, 200, 1, "Blue", "Red")];
        assert_eq!(cluster_def(&short).len(), 1, "a < break_gap lull stays one battle");
    }

    #[test]
    fn stray_kills_do_not_bridge_dense_segments() {
        let times = [0, 30, 60, 350, 640, 930, 1220, 1250, 1280];
        let engs: Vec<Engagement> =
            times.iter().enumerate().map(|(i, &t)| eng(i as i64 + 1, t, 1, "Red", "Blue")).collect();
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 2, "density valley should split the strays-bridged bursts");
        assert!(battles.iter().all(|b| b.kills >= DENSE_MIN), "a split side is not dense");
    }

    #[test]
    fn single_stray_does_not_split_a_dense_fight() {
        let times = [0, 30, 60, 90, 120, 150, 400];
        let engs: Vec<Engagement> =
            times.iter().enumerate().map(|(i, &t)| eng(i as i64 + 1, t, 1, "Red", "Blue")).collect();
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 1, "a single stray split a dense fight");
        assert_eq!(battles[0].kills, 7);
    }

    #[test]
    fn chase_merges_across_gap_same_pilots() {
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
        assert!(b.suggested_splits.iter().any(|s| s.time == 90), "splits {:?}", b.suggested_splits);
    }

    #[test]
    fn ambiguity_flags_near_threshold_gap() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 240, 1, "Blue", "Red")];
        let battles = cluster_def(&engs);
        assert_eq!(battles.len(), 1);
        assert!(battles[0].ambiguous, "near-threshold lull should be ambiguous");
        assert!(battles[0].suggested_splits.iter().any(|s| s.time == 240));
    }

    #[test]
    fn battle_report_doc_round_trips_through_json() {
        let engs = vec![
            eng(1, 0, 1, "Red", "Blue"),
            eng(2, 60, 1, "Blue", "Red"),
            eng_multi(3, 120, 1, "Red", &["Blue", "Green"]),
        ];
        let battle = preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        assert!(battle.is_two_sided());

        let mut overrides = Overrides::default();
        overrides.tag.insert(1, 7);
        overrides.excluded.insert(42);
        overrides.scrubs.insert((3, pid("Green")));

        let ship_names: std::collections::BTreeMap<i64, String> =
            [(587, "Rifter".to_owned()), (670, "Capsule".to_owned())].into_iter().collect();

        let affiliations: std::collections::BTreeMap<i64, Affil> = [
            (
                100,
                Affil {
                    corp_id: 200,
                    corp_name: "Red Corp".to_owned(),
                    alliance_id: 300,
                    alliance_name: "Red Alliance".to_owned(),
                },
            ),
            (
                101,
                Affil { corp_id: 201, corp_name: "Blue Corp".to_owned(), ..Default::default() },
            ),
        ]
        .into_iter()
        .collect();

        let doc = BattleReportDoc::new(
            battle.clone(),
            engs.clone(),
            overrides.clone(),
            Some("S1 brawl".to_owned()),
            1_700_000_000,
            ship_names.clone(),
            affiliations.clone(),
        );
        let json = doc.to_json().expect("serialize");
        let back = BattleReportDoc::from_json(&json).expect("deserialize");

        assert_eq!(back.format_version, BattleReportDoc::FORMAT_VERSION);
        assert_eq!(back.generator, concat!("eve-spai/", env!("CARGO_PKG_VERSION")));
        assert_eq!(back.exported_at, 1_700_000_000);
        assert_eq!(back.title.as_deref(), Some("S1 brawl"));
        assert_eq!(back.battle, battle);
        assert_eq!(back.engagements, engs);
        assert_eq!(back.overrides, overrides);
        assert_eq!(back.ship_names, ship_names);
        assert_eq!(back.affiliations, affiliations);
        assert_eq!(back.battle.sides.len(), battle.sides.len());
    }

    #[test]
    fn battle_report_doc_rejects_unknown_format_version() {
        let battle = preview_battle(vec![eng(1, 0, 1, "Red", "Blue")], BATTLE_BREAK_SECS);
        let mut doc = BattleReportDoc::new(
            battle,
            Vec::new(),
            Overrides::default(),
            None,
            0,
            Default::default(),
            Default::default(),
        );
        doc.format_version = 999;
        let json = doc.to_json().unwrap();
        assert!(
            BattleReportDoc::from_json(&json).is_err(),
            "an unknown format_version must be rejected"
        );
    }
}
