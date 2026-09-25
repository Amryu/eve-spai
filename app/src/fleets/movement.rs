//! Where a tracked fleet and every pilot in it went, and when.
//!
//! Each member snapshot is diffed against the last one: joins, leaves, system moves and ship swaps
//! become timestamped events, and the system holding most of the fleet is followed as the bulk.
//! Replaying the events up to a moment gives everyone's system and ship at that moment.

use std::collections::{BTreeMap, HashMap};

use crate::fleets::model::Member;

/// How a pilot or the bulk got from one system to the next, as far as two snapshots can tell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Via {
    Gate,
    Ansiblex,
    Wormhole,
    /// Not neighbours, but in jump range: a capital jump or a titan or black ops bridge.
    Jump,
    /// Several gates between two snapshots.
    Gates(u32),
    /// Too far for gates or a jump and no known hole: most likely a wormhole nobody reported.
    Unknown,
}

impl Via {
    pub fn key(self) -> String {
        match self {
            Via::Gate => "gate".into(),
            Via::Ansiblex => "ansiblex".into(),
            Via::Wormhole => "wormhole".into(),
            Via::Jump => "jump".into(),
            Via::Gates(n) => format!("gates:{n}"),
            Via::Unknown => "unknown".into(),
        }
    }

    pub fn parse(s: &str) -> Option<Via> {
        Some(match s {
            "gate" => Via::Gate,
            "ansiblex" => Via::Ansiblex,
            "wormhole" => Via::Wormhole,
            "jump" => Via::Jump,
            "unknown" => Via::Unknown,
            _ => Via::Gates(s.strip_prefix("gates:")?.parse().ok()?),
        })
    }

    pub fn label(self) -> String {
        match self {
            Via::Gate => "gate".into(),
            Via::Ansiblex => "ansiblex".into(),
            Via::Wormhole => "wormhole".into(),
            Via::Jump => "jump or bridge".into(),
            Via::Gates(n) => format!("{n} gates"),
            Via::Unknown => "unknown, likely a wormhole".into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Join,
    Leave,
    Move,
    Ship,
    /// The system holding most of the fleet changed.
    Bulk,
    /// Recording picked up again after a gap (the app was closed): what happened in between is not
    /// known.
    Resume,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Join => "join",
            Kind::Leave => "leave",
            Kind::Move => "move",
            Kind::Ship => "ship",
            Kind::Bulk => "bulk",
            Kind::Resume => "resume",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "join" => Kind::Join,
            "leave" => Kind::Leave,
            "move" => Kind::Move,
            "ship" => Kind::Ship,
            "bulk" => Kind::Bulk,
            "resume" => Kind::Resume,
            _ => return None,
        })
    }
}

/// One thing that happened. `system_id` is where the pilot (or the bulk) is after it, 0 when the
/// source did not say.
#[derive(Clone, PartialEq, Debug)]
pub struct MoveEvent {
    pub at: i64,
    pub kind: Kind,
    pub character_id: i64,
    pub name: String,
    pub system_id: i64,
    pub from_system: i64,
    pub ship_type_id: i64,
    pub ship_name: String,
    pub via: Option<Via>,
    /// Bulk events: how many pilots are in the new system.
    pub count: u32,
}

impl MoveEvent {
    pub fn new(at: i64, kind: Kind) -> Self {
        MoveEvent {
            at,
            kind,
            character_id: 0,
            name: String::new(),
            system_id: 0,
            from_system: 0,
            ship_type_id: 0,
            ship_name: String::new(),
            via: None,
            count: 0,
        }
    }
}

/// A pilot as the last snapshot left them.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Seen {
    pub name: String,
    pub system_id: i64,
    pub ship_type_id: i64,
    pub ship_name: String,
}

/// Turns snapshots into events.
#[derive(Default)]
pub struct Recorder {
    pub last: BTreeMap<i64, Seen>,
    pub bulk: i64,
}

/// A system needs this share of the located pilots, and at least two, to be where the fleet is.
const BULK_SHARE: f64 = 0.4;

impl Recorder {
    /// Picks up where earlier recording left off, so a restart does not read as everyone joining.
    pub fn resume(events: &[MoveEvent]) -> Self {
        let last = state_at(events, i64::MAX);
        let bulk = events.iter().rev().find(|e| e.kind == Kind::Bulk).map_or(0, |e| e.system_id);
        Recorder { last, bulk }
    }

    pub fn step(&mut self, members: &[&Member], at: i64, classify: &mut dyn FnMut(i64, i64) -> Via) -> Vec<MoveEvent> {
        let mut out = Vec::new();
        // An empty tree is what the hub pushes while it reconnects or right as a fleet closes; read
        // literally it would be everyone leaving and joining again a moment later.
        if members.is_empty() {
            return out;
        }
        let mut now: BTreeMap<i64, Seen> = BTreeMap::new();
        for m in members {
            now.insert(
                m.character_id,
                Seen {
                    name: m.name.clone(),
                    system_id: m.solar_system_id,
                    ship_type_id: m.ship_type_id,
                    ship_name: m.ship_type_name.clone(),
                },
            );
        }
        for (id, s) in &now {
            let ev = |kind| MoveEvent {
                character_id: *id,
                name: s.name.clone(),
                system_id: s.system_id,
                ship_type_id: s.ship_type_id,
                ship_name: s.ship_name.clone(),
                ..MoveEvent::new(at, kind)
            };
            match self.last.get(id) {
                None => out.push(ev(Kind::Join)),
                Some(was) => {
                    // A system the source stopped reporting is not a move to nowhere.
                    if s.system_id != 0 && s.system_id != was.system_id {
                        let via = (was.system_id != 0).then(|| classify(was.system_id, s.system_id));
                        out.push(MoveEvent { from_system: was.system_id, via, ..ev(Kind::Move) });
                    }
                    if s.ship_type_id != 0 && s.ship_type_id != was.ship_type_id {
                        out.push(ev(Kind::Ship));
                    }
                }
            }
        }
        for (id, was) in &self.last {
            if !now.contains_key(id) {
                out.push(MoveEvent {
                    character_id: *id,
                    name: was.name.clone(),
                    system_id: was.system_id,
                    ship_type_id: was.ship_type_id,
                    ship_name: was.ship_name.clone(),
                    ..MoveEvent::new(at, Kind::Leave)
                });
            }
        }
        // A pilot whose system went unreported keeps the last one known.
        for (id, s) in now.iter_mut() {
            if s.system_id == 0 {
                if let Some(was) = self.last.get(id) {
                    s.system_id = was.system_id;
                }
            }
        }
        if let Some((sys, n)) = bulk_of(&now) {
            if sys != self.bulk {
                let via = (self.bulk != 0).then(|| classify(self.bulk, sys));
                out.push(MoveEvent { system_id: sys, from_system: self.bulk, via, count: n, ..MoveEvent::new(at, Kind::Bulk) });
                self.bulk = sys;
            }
        }
        self.last = now;
        out
    }
}

impl Recorder {
    /// The fleet closed: everyone still in it leaves at `at`.
    pub fn close(&mut self, at: i64) -> Vec<MoveEvent> {
        let out = std::mem::take(&mut self.last)
            .into_iter()
            .map(|(id, was)| MoveEvent {
                character_id: id,
                name: was.name,
                system_id: was.system_id,
                ship_type_id: was.ship_type_id,
                ship_name: was.ship_name,
                ..MoveEvent::new(at, Kind::Leave)
            })
            .collect();
        self.bulk = 0;
        out
    }
}

/// The system holding the fleet, and how many are there. None when the fleet is spread out.
fn bulk_of(pilots: &BTreeMap<i64, Seen>) -> Option<(i64, u32)> {
    let mut counts: HashMap<i64, u32> = HashMap::new();
    for s in pilots.values().filter(|s| s.system_id != 0) {
        *counts.entry(s.system_id).or_default() += 1;
    }
    let located: u32 = counts.values().sum();
    // Ties go to the lower id so the answer does not flicker between equal halves.
    let (sys, n) = counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?;
    (n >= 2 && n as f64 >= located as f64 * BULK_SHARE).then_some((sys, n))
}

/// Kills and losses the fleet had a part in. The dashboard's roster is everyone who was ever in
/// it, so a pilot who joined for a minute otherwise brought their kills from the other side of the
/// map for the rest of the run. A kill counts when one of its pilots was in the fleet then, when
/// the fleet had pilots in that system then, or when its pilot dropped out of the fleet in that
/// system shortly before (a disconnect or a kick mid-fight). With no recorded movement nothing is
/// known about who was where, and everything is kept.
pub fn in_fleet_kills(kills: &[crate::store::FleetKill], events: &[MoveEvent]) -> Vec<crate::store::FleetKill> {
    /// A join or a leave is only seen at the next read; a kill that close to one is given the doubt.
    const GRACE: i64 = 60;
    /// How long after dropping out a pilot dying or killing where they left still counts.
    const DROPPED: i64 = 600;
    if events.is_empty() {
        return kills.to_vec();
    }
    kills
        .iter()
        .filter(|k| {
            let now = state_at(events, k.at);
            let around = [state_at(events, k.at - GRACE), state_at(events, k.at + GRACE)];
            let member = k.members.iter().any(|m| now.contains_key(m) || around.iter().any(|s| s.contains_key(m)));
            let fleet_here = now.values().any(|s| s.system_id == k.system_id);
            let dropped_here = k.members.iter().any(|m| {
                events
                    .iter()
                    .filter(|e| e.character_id == *m && e.at <= k.at)
                    .last()
                    .is_some_and(|e| e.kind == Kind::Leave && k.at - e.at <= DROPPED && e.system_id == k.system_id)
            });
            member || fleet_here || dropped_here
        })
        .cloned()
        .collect()
}

pub fn is_pod(ship_type_id: i64) -> bool {
    br_core::battle::POD_TYPES.contains(&ship_type_id)
}

/// Everyone in the fleet at `t`, with the system and ship they had then.
pub fn state_at(events: &[MoveEvent], t: i64) -> BTreeMap<i64, Seen> {
    let mut out: BTreeMap<i64, Seen> = BTreeMap::new();
    for e in events.iter().filter(|e| e.at <= t) {
        match e.kind {
            Kind::Join => {
                out.insert(
                    e.character_id,
                    Seen { name: e.name.clone(), system_id: e.system_id, ship_type_id: e.ship_type_id, ship_name: e.ship_name.clone() },
                );
            }
            Kind::Leave => {
                out.remove(&e.character_id);
            }
            Kind::Move => {
                if let Some(s) = out.get_mut(&e.character_id) {
                    s.system_id = e.system_id;
                }
            }
            Kind::Ship => {
                if let Some(s) = out.get_mut(&e.character_id) {
                    s.ship_type_id = e.ship_type_id;
                    s.ship_name = e.ship_name.clone();
                }
            }
            Kind::Bulk | Kind::Resume => {}
        }
    }
    out
}

/// How a move from `a` to `b` was most likely made.
///
/// `hole` answers whether a known wormhole links the two. J-space on either end is a hole by
/// definition. Past gate and bridge neighbours, anything within `jump_ly` is a jump or a bridge, a
/// short gate path is several gates between snapshots, and the rest is a hole nobody reported.
pub fn classify(
    graph: &crate::geo::Systems,
    a: i64,
    b: i64,
    jump_ly: f64,
    hole: &dyn Fn(i64, i64) -> bool,
) -> Via {
    let jspace = |id: i64| (31_000_000..32_000_000).contains(&id);
    if jspace(a) || jspace(b) || hole(a, b) {
        return Via::Wormhole;
    }
    if graph.neighbors_gates_only(a).contains(&b) {
        return Via::Gate;
    }
    if graph.is_bridge(a, b) {
        return Via::Ansiblex;
    }
    if graph.ly_between(a, b).is_some_and(|ly| ly <= jump_ly) {
        return Via::Jump;
    }
    match graph.jumps(a, b, MAX_GATES_BETWEEN) {
        Some(n) => Via::Gates(n),
        None => Via::Unknown,
    }
}

/// More gates than this between two snapshots a few seconds apart did not happen by gate.
const MAX_GATES_BETWEEN: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: i64, sys: i64, ship: i64) -> Member {
        Member {
            character_id: id,
            name: format!("Pilot {id}"),
            ship_type_id: ship,
            ship_type_name: format!("Hull {ship}"),
            solar_system_id: sys,
            ..Member::default()
        }
    }

    fn step(r: &mut Recorder, members: &[Member], at: i64) -> Vec<MoveEvent> {
        let refs: Vec<&Member> = members.iter().collect();
        r.step(&refs, at, &mut |_, _| Via::Gate)
    }

    fn kinds(ev: &[MoveEvent]) -> Vec<(Kind, i64)> {
        ev.iter().map(|e| (e.kind, e.character_id)).collect()
    }

    #[test]
    fn joins_moves_swaps_and_leaves_become_events() {
        let mut r = Recorder::default();
        let first = step(&mut r, &[m(1, 10, 100), m(2, 10, 100), m(3, 20, 100)], 1000);
        assert_eq!(kinds(&first), vec![(Kind::Join, 1), (Kind::Join, 2), (Kind::Join, 3), (Kind::Bulk, 0)]);
        assert_eq!((first[3].system_id, first[3].count), (10, 2));

        let second = step(&mut r, &[m(1, 11, 100), m(2, 11, 200), m(4, 11, 100)], 1060);
        assert_eq!(
            kinds(&second),
            vec![(Kind::Move, 1), (Kind::Move, 2), (Kind::Ship, 2), (Kind::Join, 4), (Kind::Leave, 3), (Kind::Bulk, 0)]
        );
        let moved = &second[0];
        assert_eq!((moved.from_system, moved.system_id, moved.via), (10, 11, Some(Via::Gate)));
        assert_eq!((second[5].from_system, second[5].system_id, second[5].count), (10, 11, 3));
        assert!(step(&mut r, &[m(1, 11, 100), m(2, 11, 200), m(4, 11, 100)], 1120).is_empty(), "nothing changed");
    }

    /// The hub pushes an empty tree while it reconnects. Reading it would log everyone leaving.
    #[test]
    fn an_empty_snapshot_is_not_everyone_leaving() {
        let mut r = Recorder::default();
        step(&mut r, &[m(1, 10, 100), m(2, 10, 100)], 1000);
        assert!(step(&mut r, &[], 1010).is_empty());
        assert!(step(&mut r, &[m(1, 10, 100), m(2, 10, 100)], 1020).is_empty());
    }

    /// A system the source stopped sending is kept, not recorded as a move to nowhere.
    #[test]
    fn an_unreported_system_keeps_the_last_known_one() {
        let mut r = Recorder::default();
        step(&mut r, &[m(1, 10, 100), m(2, 10, 100)], 1000);
        assert!(step(&mut r, &[m(1, 0, 100), m(2, 10, 100)], 1010).is_empty());
        let ev = step(&mut r, &[m(1, 11, 100), m(2, 10, 100)], 1020);
        assert_eq!(ev.iter().map(|e| (e.kind, e.from_system)).collect::<Vec<_>>(), vec![(Kind::Move, 10)]);
    }

    #[test]
    fn replay_answers_where_everyone_was() {
        let mut r = Recorder::default();
        let mut log = step(&mut r, &[m(1, 10, 100), m(2, 10, 100)], 1000);
        log.extend(step(&mut r, &[m(1, 11, 200)], 1100));
        let then = state_at(&log, 1050);
        assert_eq!(then.len(), 2);
        assert_eq!(then[&1].system_id, 10);
        let later = state_at(&log, 1100);
        assert_eq!(later.len(), 1, "pilot 2 left");
        assert_eq!((later[&1].system_id, later[&1].ship_type_id), (11, 200));
        // Resuming from the log is not everyone joining again.
        let mut again = Recorder::resume(&log);
        assert!(step(&mut again, &[m(1, 11, 200)], 1200).is_empty());
    }

    /// A fleet split evenly has no bulk, and a lone pilot is not the fleet.
    #[test]
    fn a_spread_fleet_has_no_bulk() {
        let mut r = Recorder::default();
        let ev = step(&mut r, &[m(1, 10, 1), m(2, 11, 1), m(3, 12, 1), m(4, 13, 1), m(5, 14, 1)], 1000);
        assert!(ev.iter().all(|e| e.kind != Kind::Bulk));
    }

    /// A pilot who joined for a minute does not bring their kills from elsewhere later in the run.
    #[test]
    fn only_kills_while_in_the_fleet_count() {
        let mut r = Recorder::default();
        let mut log = step(&mut r, &[m(1, 10, 100), m(2, 10, 100)], 1000);
        log.extend(step(&mut r, &[m(1, 10, 100)], 1100));
        let kill = |at: i64, who: i64, sys: i64| crate::store::FleetKill { kill_id: at, at, system_id: sys, members: vec![who], ..Default::default() };
        // Pilot 2 left at 1100 in system 10; pilot 1 stays in 10.
        let kept = in_fleet_kills(
            &[kill(1050, 2, 20), kill(9000, 2, 20), kill(9000, 1, 20), kill(1400, 2, 10), kill(9000, 3, 10), kill(9000, 2, 30)],
            &log,
        );
        assert_eq!(
            kept.iter().map(|k| (k.at, k.members[0], k.system_id)).collect::<Vec<_>>(),
            vec![(1050, 2, 20), (9000, 1, 20), (1400, 2, 10), (9000, 3, 10)],
            "in the fleet, a member, dropped out here moments ago, or where the fleet is; not elsewhere later"
        );
        assert_eq!(in_fleet_kills(&[kill(9000, 2, 30)], &[]).len(), 1, "nothing recorded, nothing to go on");
    }

    #[test]
    fn moves_are_classified_by_what_connects_the_systems() {
        let mut by_name = HashMap::new();
        for (id, name) in [(1, "A"), (2, "B"), (3, "C"), (4, "D"), (5, "E")] {
            by_name.insert(
                name.to_lowercase(),
                crate::geo::SystemInfo {
                    id,
                    name: name.into(),
                    security: -0.4,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                },
            );
        }
        let adjacency = HashMap::from([(1, vec![2]), (2, vec![1, 3]), (3, vec![2])]);
        let mut g = crate::geo::Systems::new(by_name, adjacency);
        let ly = crate::map::LY_METERS;
        g.set_positions(HashMap::from([
            (1, [0.0, 0.0, 0.0]),
            (2, [1.0 * ly, 0.0, 0.0]),
            (3, [2.0 * ly, 0.0, 0.0]),
            (4, [5.0 * ly, 0.0, 0.0]),
            (5, [40.0 * ly, 0.0, 0.0]),
        ]));
        g.add_bridges(&[(1, 5)]);
        let none = |_: i64, _: i64| false;
        assert_eq!(classify(&g, 1, 2, 8.0, &none), Via::Gate);
        assert_eq!(classify(&g, 1, 5, 8.0, &none), Via::Ansiblex);
        assert_eq!(classify(&g, 1, 4, 8.0, &none), Via::Jump);
        assert_eq!(classify(&g, 1, 31_000_005, 8.0, &none), Via::Wormhole);
        assert_eq!(classify(&g, 2, 4, 8.0, &|a, b| (a, b) == (2, 4)), Via::Wormhole);
        assert_eq!(classify(&g, 1, 3, 1.5, &none), Via::Gates(2));
        assert_eq!(classify(&g, 3, 5, 1.5, &none), Via::Gates(3), "via the bridge");
        assert_eq!(Via::parse(&Via::Gates(3).key()), Some(Via::Gates(3)));
    }
}
