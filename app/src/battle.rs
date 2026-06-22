//! Battle-report clustering (docs/DESIGN.md §7.2).
//!
//! Groups killmails ("engagements") into battles: two engagements belong to the
//! same battle if they are within `window` seconds AND `max_jumps` jumps of each
//! other (transitively — a battle chains across systems/time). Jump distance uses
//! the geo graph, which already includes configured jump bridges.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// 10 minutes between linked engagements.
pub const BATTLE_WINDOW_SECS: i64 = 600;
/// Up to 3 jumps (gates or configured bridges) between linked engagements.
pub const BATTLE_MAX_JUMPS: u32 = 3;

#[allow(dead_code)] // Faction is for future faction-warfare kills
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartyKind {
    Alliance,
    Corporation,
    Character,
    Faction,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Party {
    /// Entity id — for future zKill/Dotlan links.
    #[allow(dead_code)]
    pub id: i64,
    pub name: String,
    /// Alliance/corp/character — for future per-party icons.
    #[allow(dead_code)]
    pub kind: PartyKind,
}

/// One killmail: a victim destroyed by attackers, in a system, at a time.
#[derive(Clone, Debug)]
pub struct Engagement {
    pub kill_id: i64,
    pub time: i64,
    pub system_id: i64,
    pub system_name: String,
    pub security: f64,
    pub victim: Party,
    pub attackers: Vec<Party>,
    pub isk: f64,
}

/// One side of a battle: parties that fought together (co-attackers).
#[derive(Clone, Debug)]
pub struct Side {
    /// Member parties, most-involved first.
    pub parties: Vec<String>,
    /// Kills scored by this side.
    pub kills: u32,
    /// Ships lost by this side.
    pub losses: u32,
    /// ISK destroyed *from* this side (its losses' value).
    pub isk_lost: f64,
}

#[derive(Clone, Debug)]
pub struct Battle {
    /// Kept for future per-kill drill-down.
    #[allow(dead_code)]
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

/// Cluster engagements into battles. `dist(a, b)` is the jump distance between two
/// systems (None if too far / unreachable).
pub fn cluster(
    engagements: &[Engagement],
    window: i64,
    max_jumps: u32,
    dist: impl Fn(i64, i64) -> Option<u32>,
) -> Vec<Battle> {
    let n = engagements.len();
    let mut uf = UnionFind::new(n);
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &engagements[i];
            let b = &engagements[j];
            // "less than `window` seconds since an engagement" — strict.
            if (a.time - b.time).abs() >= window {
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

/// Split parties into belligerent sides: co-attackers on a kill are allied; a
/// victim opposes its attackers. Union co-attackers, then aggregate per component.
fn infer_sides(engs: &[Engagement]) -> Vec<Side> {
    // Index every distinct party name.
    let mut index: HashMap<String, usize> = HashMap::new();
    for e in engs {
        for name in std::iter::once(&e.victim.name).chain(e.attackers.iter().map(|a| &a.name)) {
            let next = index.len();
            index.entry(name.clone()).or_insert(next);
        }
    }
    let mut uf = UnionFind::new(index.len());
    for e in engs {
        let mut attackers: Vec<usize> = e.attackers.iter().map(|a| index[&a.name]).collect();
        attackers.dedup();
        for w in attackers.windows(2) {
            uf.union(w[0], w[1]);
        }
    }

    // root -> (members set, kills, losses, isk_lost)
    let mut sides: HashMap<usize, (BTreeSet<String>, HashMap<String, u32>, u32, u32, f64)> =
        HashMap::new();
    let mut root_of: HashMap<String, usize> = HashMap::new();
    for (name, &i) in &index {
        let r = uf.find(i);
        root_of.insert(name.clone(), r);
        sides.entry(r).or_default().0.insert(name.clone());
    }
    for e in engs {
        // Victim's side takes a loss.
        let vr = root_of[&e.victim.name];
        let s = sides.get_mut(&vr).unwrap();
        s.3 += 1;
        s.4 += e.isk;
        *s.1.entry(e.victim.name.clone()).or_default() += 1;
        // Each attacking side scores one kill.
        let mut scored: BTreeSet<usize> = BTreeSet::new();
        for a in &e.attackers {
            let r = root_of[&a.name];
            *sides.get_mut(&r).unwrap().1.entry(a.name.clone()).or_default() += 1;
            if scored.insert(r) {
                sides.get_mut(&r).unwrap().2 += 1;
            }
        }
    }

    let mut out: Vec<Side> = sides
        .into_values()
        .map(|(members, involvement, kills, losses, isk_lost)| {
            let mut parties: Vec<String> = members.into_iter().collect();
            parties.sort_by_key(|p| std::cmp::Reverse(involvement.get(p).copied().unwrap_or(0)));
            Side {
                parties,
                kills,
                losses,
                isk_lost,
            }
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.kills + s.losses));
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

    fn eng(kill: i64, time: i64, sys: i64, victim: &str, attacker: &str) -> Engagement {
        Engagement {
            kill_id: kill,
            time,
            system_id: sys,
            system_name: format!("S{sys}"),
            security: 0.0,
            victim: party(1, victim),
            attackers: vec![party(2, attacker)],
            isk: 1.0,
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
        let blue = b.sides.iter().find(|s| s.parties.contains(&"Blue".to_string())).unwrap();
        let red = b.sides.iter().find(|s| s.parties.contains(&"Red".to_string())).unwrap();
        assert_eq!((blue.kills, blue.losses), (2, 0));
        assert_eq!((red.kills, red.losses), (0, 2));
    }

    #[test]
    fn coattackers_form_one_side() {
        // Blue + Green kill Red together -> one side {Blue, Green} vs {Red}.
        let mut e = eng(1, 0, 1, "Red", "Blue");
        e.attackers.push(party(3, "Green"));
        let battles = cluster(std::slice::from_ref(&e), BATTLE_WINDOW_SECS, BATTLE_MAX_JUMPS, dist);
        let b = &battles[0];
        assert_eq!(b.sides.len(), 2);
        let allied = b.sides.iter().find(|s| s.parties.contains(&"Blue".to_string())).unwrap();
        assert!(allied.parties.contains(&"Green".to_string()));
        assert_eq!(allied.kills, 1);
    }
}
