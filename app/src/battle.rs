//! Battle-report clustering (docs/DESIGN.md §7.2).
//!
//! Groups killmails ("engagements") into battles: two engagements belong to the
//! same battle if they are within `window` seconds AND `max_jumps` jumps of each
//! other (transitively — a battle chains across systems/time). Jump distance uses
//! the geo graph, which already includes configured jump bridges.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 10 minutes between linked engagements.
pub const BATTLE_WINDOW_SECS: i64 = 600;
/// Up to 3 jumps (gates or configured bridges) between linked engagements.
pub const BATTLE_MAX_JUMPS: u32 = 3;

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

impl Participant {
    fn value(&self) -> f64 {
        self.lost.as_ref().map_or(0.0, |l| l.value)
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

        // Group by hull type; within a hull, destroyed first (highest value), then survivors.
        parts.sort_by(|a, b| {
            a.ship
                .cmp(&b.ship)
                .then(b.lost.is_some().cmp(&a.lost.is_some()))
                .then(b.value().total_cmp(&a.value()))
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
    dist: impl Fn(i64, i64) -> Option<u32>,
) -> Vec<Battle> {
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
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &engagements[i];
            let b = &engagements[j];
            // "less than `window` seconds since an engagement" — strict.
            if (a.time - b.time).abs() >= window {
                continue;
            }
            // Shared participant is the cheap, deciding test — apply it before the (potentially
            // expensive) jump-distance BFS, and skip pairs already in the same battle.
            if parties[i].is_disjoint(&parties[j]) || uf.find(i) == uf.find(j) {
                continue;
            }
            let close = a.system_id == b.system_id
                || dist(a.system_id, b.system_id).is_some_and(|d| d <= max_jumps);
            if close {
                uf.union(i, j);
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        groups.entry(uf.find(i)).or_default().push(i);
    }

    let mut battles: Vec<Battle> = groups
        .into_values()
        .map(|idxs| build_battle(idxs.iter().map(|&i| engagements[i].clone()).collect()))
        .collect();
    // Newest battles first.
    battles.sort_by(|a, b| b.end.cmp(&a.end));
    battles
}

fn build_battle(mut engs: Vec<Engagement>) -> Battle {
    engs.sort_by_key(|e| e.time);
    let start = engs.first().map_or(0, |e| e.time);
    let end = engs.last().map_or(0, |e| e.time);

    let mut systems: BTreeMap<i64, (String, f64)> = BTreeMap::new();
    let mut isk = 0.0;
    for e in &engs {
        systems.insert(e.system_id, (e.system_name.clone(), e.security));
        isk += e.isk;
    }

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
        engagements: engs,
    }
}

/// Number of kills between two parties (either direction) that marks a *real* fight rather than
/// stray friendly fire — below this they may end up on the same side.
const SIGNIF: u32 = 2;

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
    let mut coattack: BTreeSet<(usize, usize)> = BTreeSet::new();
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
                coattack.insert((atk[i], atk[j]));
            }
        }
    }
    let mutual = |a: usize, b: usize| {
        hostility.get(&(a, b)).copied().unwrap_or(0) + hostility.get(&(b, a)).copied().unwrap_or(0)
    };

    // 1) Union co-attackers that aren't significant enemies of each other.
    let mut uf = UnionFind::new(n.max(1));
    for &(a, b) in &coattack {
        if mutual(a, b) < SIGNIF {
            uf.union(a, b);
        }
    }
    // 2) Merge components that share a common enemy and aren't enemies of each other (repeat to
    //    a fixpoint so "A fought X, B fought X" chains collapse into one side).
    loop {
        // Two enemy views per component: `hard` (fought ≥ SIGNIF, marks genuine opponents we must
        // not merge) and `any` (shot at all, a shared aggression target). Groups that share an
        // aggression target and didn't significantly fight each other are the same side — this is
        // what merges alliance-less players (each in their own NPC corp) who each landed a hit or
        // two on the same enemy, instead of splitting them into many one-corp "sides".
        let mut hard: HashMap<usize, BTreeSet<usize>> = HashMap::new();
        let mut any: HashMap<usize, BTreeSet<usize>> = HashMap::new();
        for a in 0..n {
            for b in 0..n {
                if a == b {
                    continue;
                }
                let m = mutual(a, b);
                if m >= 1 {
                    any.entry(uf.find(a)).or_default().insert(uf.find(b));
                }
                if m >= SIGNIF {
                    hard.entry(uf.find(a)).or_default().insert(uf.find(b));
                }
            }
        }
        let roots: Vec<usize> = (0..n).map(|i| uf.find(i)).collect::<BTreeSet<_>>().into_iter().collect();
        let mut merged = false;
        'outer: for i in 0..roots.len() {
            for j in (i + 1)..roots.len() {
                let (ra, rb) = (roots[i], roots[j]);
                let ha = hard.get(&ra).cloned().unwrap_or_default();
                let hb = hard.get(&rb).cloned().unwrap_or_default();
                let are_enemies = ha.contains(&rb) || hb.contains(&ra);
                let aa = any.get(&ra).cloned().unwrap_or_default();
                let ab = any.get(&rb).cloned().unwrap_or_default();
                let common_enemy = aa.intersection(&ab).next().is_some();
                if !are_enemies && common_enemy {
                    uf.union(ra, rb);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            break;
        }
    }
    let root: Vec<usize> = (0..n).map(|i| uf.find(i)).collect();

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
        // A@sys1 t=0, B@sys2 t=300 (1 jump, 5 min) -> same battle (chained).
        // C@sys4 t=1000 is >10 min after both -> separate battle.
        let engs = [
            eng(1, 0, 1, "Red", "Blue"),
            eng(2, 300, 2, "Blue", "Red"),
            eng(3, 1000, 4, "Green", "Blue"),
        ];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
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
        let anchored = eng(2, 420, 1, "Rorqual", "Blue"); // 7 min later, same system, shares Blue
        let battles = cluster(&[unanchored, anchored], BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
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
        let battles = cluster(&[e1, e2], BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
        assert_eq!(battles.len(), 1);
        assert!(!battles[0].is_anchored(), "fully out-of-area battle must be filtered out");
        // The post-cluster filter the app applies:
        assert_eq!(battles.into_iter().filter(|b| b.is_anchored()).count(), 0);
    }

    #[test]
    fn unrelated_fights_do_not_merge() {
        // Same system, same time, but disjoint belligerents — two separate battles.
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 30, 1, "Green", "Yellow")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
        assert_eq!(battles.len(), 2, "unrelated fights merged into one BR");
    }

    #[test]
    fn shared_party_chains_across_systems() {
        // Fights in different systems sharing Blue chain into one battle.
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 2, "Green", "Blue")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
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
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
        assert_eq!(battles.len(), 1);
        assert_eq!(battles[0].kills, 2);
    }

    #[test]
    fn sides_split_by_kills_and_losses() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 1, "Red", "Blue")];
        let battles = cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
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
        let battles = cluster(std::slice::from_ref(&e), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
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
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
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
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
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
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
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
        let b = &cluster(std::slice::from_ref(&k1), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
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
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];

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
    fn roster_groups_by_hull_lost_first() {
        // Same hull (24692): one lost (Red), one survivor (Blue who also flew 24692). Plus Blue
        // loses a different hull. Expect grouping by ship type, destroyed before survived.
        let v = |kill, time, victim: &str, ship, attacker: &str, isk| Engagement {
            victim_ship: ship,
            isk,
            ..eng(kill, time, 1, victim, attacker)
        };
        // Blue (attacker, ship unknown here) kills Red's Abaddon(24692) and Red's Rifter(587).
        let engs = [v(1, 0, "Red", 24692, "Blue", 100.0), v(2, 30, "Red", 587, "Blue", 5.0)];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
        let red = b.side_of(&party(pid("Red"), "Red")).unwrap();
        let roster = b.roster(red);
        // Both are Red losses, grouped ascending by ship type id (587 then 24692).
        let ships: Vec<i64> = roster.iter().map(|p| p.ship).collect();
        assert_eq!(ships, vec![587, 24692]);
        assert!(roster.iter().all(|p| p.lost.is_some()));
    }

    #[test]
    fn matches_filters_by_system_and_pilot() {
        let engs = [eng(1, 0, 1, "Red", "Blue"), eng(2, 60, 1, "Red", "Blue")];
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
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
        let b = &cluster(&engs, BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
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
        let b = &cluster(std::slice::from_ref(&e), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist)[0];
        let imp = b.sides.iter().find(|s| s.parties.iter().any(|p| p.id == 1354830081)).unwrap();
        assert_eq!(imp.coalition.as_deref(), Some("The Imperium"));
        assert!(imp.parties.iter().any(|p| p.id == 99010079));
    }
}
