//! Whether one of our own characters just went through a wormhole, from how its system changed.
//!
//! A jump the gates cannot explain is a hole, a filament, a clone, a death or a capital jump. The
//! other explanations are ruled out first; what is left is sorted by what could connect the two
//! systems: any J-space end is certainly a hole, Pochven is one when the k-space end is in that
//! system's C729 zone, and k-space to k-space is only possibly one (S199 and the like are rare, and
//! filaments land in the same places).

use crate::whdata::{self, Candidate, Class};

/// A character's system changed between two location polls.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub character: String,
    pub from: i64,
    pub to: i64,
    pub at: i64,
    /// Seconds since the character was last seen in `from`.
    pub gap_secs: i64,
    pub ship_before: Option<i64>,
    pub ship_after: Option<i64>,
    /// The hull group after the move, e.g. "Titan" or "Black Ops".
    pub group_after: Option<String>,
    pub docked_after: bool,
    /// The last jump drive jump or bridge, from jump fatigue, when it could be read.
    pub last_jump: Option<i64>,
}

/// What the character's clones say about where it can appear without flying there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clones {
    pub home_system: Option<i64>,
    pub jump_clone_systems: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Explained {
    Gates(u32),
    Abyss,
    Death,
    JumpClone,
    CapitalJump,
    /// Jump fatigue shows a jump drive jump or a bridge in the window of the move.
    Jumped,
    /// No hole type connects the two systems' kinds of space.
    NoHoleFits,
    /// Pochven reached from outside its C729 zone: a Pochven filament.
    Filament,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    /// Not a hole, and why.
    Explained(Explained),
    /// Certainly a hole; these types could have been it.
    Hole(Vec<Candidate>),
    /// Maybe a hole: a filament or the like fits as well.
    Possible(Vec<Candidate>),
}

/// Where a system sits for wormhole purposes: its id, security and region name.
pub trait SystemFacts {
    fn facts(&self, id: i64) -> Option<(f64, String, String)>;
}

impl SystemFacts for crate::geo::Systems {
    fn facts(&self, id: i64) -> Option<(f64, String, String)> {
        self.info_of(id).map(|i| (i.security, i.region.clone(), i.name.clone()))
    }
}

/// Seconds a gate jump takes at the least, warp and session change included. A gap that could
/// hold this many jumps' worth of travel is walked by gates before a hole is considered.
const SECS_PER_GATE: i64 = 12;
const CAPITAL_GROUPS: [&str; 8] =
    ["Titan", "Supercarrier", "Carrier", "Dreadnought", "Force Auxiliary", "Black Ops", "Jump Freighter", "Capital Industrial Ship"];
/// Capsule and Genolution capsule.
const PODS: [i64; 2] = [670, 33_328];

fn is_abyss(id: i64) -> bool {
    (32_000_000..33_000_000).contains(&id)
}

pub fn classify(t: &Transition, geo: &crate::geo::Systems, clones: &Clones) -> Verdict {
    classify_with(t, geo, geo, clones)
}

pub fn classify_with(t: &Transition, facts: &dyn SystemFacts, geo: &crate::geo::Systems, clones: &Clones) -> Verdict {
    if is_abyss(t.from) || is_abyss(t.to) || t.from == t.to {
        return Verdict::Explained(Explained::Abyss);
    }
    let reach = ((t.gap_secs / SECS_PER_GATE) as u32).clamp(1, 60);
    if let Some(n) = geo.jumps(t.from, t.to, reach) {
        return Verdict::Explained(Explained::Gates(n));
    }
    let in_pod = t.ship_after.is_some_and(|s| PODS.contains(&s));
    if clones.home_system == Some(t.to) && (in_pod || t.docked_after) {
        return Verdict::Explained(Explained::Death);
    }
    let ship_changed = t.ship_before.is_some() && t.ship_after.is_some() && t.ship_before != t.ship_after;
    if t.docked_after && ship_changed && clones.jump_clone_systems.contains(&t.to) {
        return Verdict::Explained(Explained::JumpClone);
    }
    let class = |id: i64| facts.facts(id).map(|(sec, region, _)| whdata::class_of(id, sec, &region));
    let (Some(cf), Some(ct)) = (class(t.from), class(t.to)) else {
        // A system the SDE does not know is past anything to judge.
        return Verdict::Possible(Vec::new());
    };
    // Slack either side: the poll times and ESI's clock need not agree to the second.
    let jumped_in_window = t.last_jump.is_some_and(|j| j >= t.at - t.gap_secs - 5 && j <= t.at + 5);
    let plain_kspace = |c: Class| matches!(c, Class::Hs | Class::Ls | Class::Ns);
    if jumped_in_window && plain_kspace(cf) && plain_kspace(ct) {
        return Verdict::Explained(Explained::Jumped);
    }
    let capital = t.group_after.as_deref().is_some_and(|g| CAPITAL_GROUPS.contains(&g));
    if capital && cf.is_kspace() && ct.is_kspace() && geo.ly_between(t.from, t.to).is_some_and(|ly| ly <= 10.0) {
        return Verdict::Explained(Explained::CapitalJump);
    }
    let candidates = whdata::possible_holes(cf, ct);
    let jspace = |c: Class| matches!(c, Class::W(_) | Class::Thera | Class::Drifter(_));
    if jspace(cf) || jspace(ct) {
        return Verdict::Hole(candidates);
    }
    // Pochven: its C729 lands only in a fixed zone per system. Anywhere else is a U372 or X450 at
    // best, and those are rare; a Pochven filament fits just as well.
    if cf == Class::Pochven || ct == Class::Pochven {
        let name = |id: i64| facts.facts(id).map(|(_, _, n)| n).unwrap_or_default();
        let (poch, other) = if cf == Class::Pochven { (t.from, t.to) } else { (t.to, t.from) };
        if cf == Class::Pochven && ct == Class::Pochven {
            return Verdict::Explained(Explained::Gates(1));
        }
        let in_zone = whdata::c729_targets(&name(other)).iter().any(|p| p.eq_ignore_ascii_case(&name(poch)));
        if in_zone {
            return Verdict::Hole(candidates.into_iter().filter(|c| c.code == "C729").collect());
        }
        let rest: Vec<Candidate> = candidates.into_iter().filter(|c| c.code != "C729").collect();
        return if rest.is_empty() { Verdict::Explained(Explained::Filament) } else { Verdict::Possible(rest) };
    }
    if candidates.is_empty() {
        return Verdict::Explained(Explained::NoHoleFits);
    }
    Verdict::Possible(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Facts(HashMap<i64, (f64, String, String)>);

    impl SystemFacts for Facts {
        fn facts(&self, id: i64) -> Option<(f64, String, String)> {
            self.0.get(&id).cloned()
        }
    }

    const JITA: i64 = 1;
    const PERIMETER: i64 = 2;
    const NULL_A: i64 = 3;
    const NULL_B: i64 = 4;
    const JSYS: i64 = 31_002_318;
    const KINO: i64 = 30_001_372;
    const ISANAMO: i64 = 5;

    fn world() -> (Facts, crate::geo::Systems) {
        let facts = Facts(HashMap::from([
            (JITA, (0.9, "The Forge".into(), "Jita".into())),
            (PERIMETER, (0.9, "The Forge".into(), "Perimeter".into())),
            (NULL_A, (-0.3, "Delve".into(), "Null A".into())),
            (NULL_B, (-0.3, "Delve".into(), "Null B".into())),
            (JSYS, (-1.0, "A-R00029".into(), "J111613".into())),
            (KINO, (-1.0, "Pochven".into(), "Kino".into())),
            (ISANAMO, (0.8, "Lonetrek".into(), "Isanamo".into())),
        ]));
        let mut by_name = HashMap::new();
        for (id, (sec, region, name)) in &facts.0 {
            by_name.insert(
                name.to_lowercase(),
                crate::geo::SystemInfo {
                    id: *id,
                    name: name.clone(),
                    security: *sec,
                    constellation: String::new(),
                    region: region.clone(),
                    faction: String::new(),
                },
            );
        }
        let geo = crate::geo::Systems::new(by_name, HashMap::from([(JITA, vec![PERIMETER]), (PERIMETER, vec![JITA])]));
        (facts, geo)
    }

    fn t(from: i64, to: i64) -> Transition {
        Transition {
            character: "Test Pilot".into(),
            from,
            to,
            at: 1000,
            gap_secs: 20,
            ship_before: Some(11_176),
            ship_after: Some(11_176),
            group_after: Some("Interceptor".into()),
            docked_after: false,
            last_jump: None,
        }
    }

    fn verdict(tr: &Transition, clones: &Clones) -> Verdict {
        let (facts, geo) = world();
        classify_with(tr, &facts, &geo, clones)
    }

    #[test]
    fn a_gate_is_not_a_hole() {
        assert_eq!(verdict(&t(JITA, PERIMETER), &Clones::default()), Verdict::Explained(Explained::Gates(1)));
    }

    #[test]
    fn j_space_on_either_end_is_a_hole() {
        assert!(matches!(verdict(&t(NULL_A, JSYS), &Clones::default()), Verdict::Hole(_)));
        assert!(matches!(verdict(&t(JSYS, JITA), &Clones::default()), Verdict::Hole(_)));
    }

    #[test]
    fn null_to_null_is_only_possibly_a_hole() {
        match verdict(&t(NULL_A, NULL_B), &Clones::default()) {
            Verdict::Possible(c) => assert!(c.iter().any(|c| c.code == "S199")),
            v => panic!("{v:?}"),
        }
    }

    #[test]
    fn a_pod_at_the_home_clone_is_a_death() {
        let mut tr = t(NULL_A, JITA);
        tr.ship_after = Some(670);
        let clones = Clones { home_system: Some(JITA), ..Default::default() };
        assert_eq!(verdict(&tr, &clones), Verdict::Explained(Explained::Death));
    }

    #[test]
    fn docking_in_another_ship_at_a_jump_clone_is_a_clone_jump() {
        let mut tr = t(NULL_A, NULL_B);
        tr.docked_after = true;
        tr.ship_after = Some(17_740);
        let clones = Clones { jump_clone_systems: vec![NULL_B], ..Default::default() };
        assert_eq!(verdict(&tr, &clones), Verdict::Explained(Explained::JumpClone));
    }

    #[test]
    fn the_abyss_and_back_is_not_a_hole() {
        assert_eq!(verdict(&t(NULL_A, 32_000_123), &Clones::default()), Verdict::Explained(Explained::Abyss));
        assert_eq!(verdict(&t(32_000_123, NULL_A), &Clones::default()), Verdict::Explained(Explained::Abyss));
    }

    #[test]
    fn pochven_is_a_hole_only_from_its_c729_zone() {
        match verdict(&t(ISANAMO, KINO), &Clones::default()) {
            Verdict::Hole(c) => assert_eq!(c.iter().map(|c| c.code).collect::<Vec<_>>(), vec!["C729"]),
            v => panic!("Isanamo hosts Kino's C729: {v:?}"),
        }
        assert_eq!(verdict(&t(JITA, KINO), &Clones::default()), Verdict::Explained(Explained::Filament), "highsec outside the zone");
    }

    #[test]
    fn a_bridge_shown_by_jump_fatigue_is_not_a_hole() {
        let mut tr = t(NULL_A, NULL_B);
        tr.last_jump = Some(tr.at - 10);
        assert_eq!(verdict(&tr, &Clones::default()), Verdict::Explained(Explained::Jumped), "a bridged subcap");
        tr.last_jump = Some(tr.at - 3600);
        assert!(matches!(verdict(&tr, &Clones::default()), Verdict::Possible(_)), "an old jump explains nothing");
        let mut into_j = t(NULL_A, JSYS);
        into_j.last_jump = Some(into_j.at - 10);
        assert!(matches!(verdict(&into_j, &Clones::default()), Verdict::Hole(_)), "nothing jumps into J-space");
    }

    #[test]
    fn a_long_gap_is_walked_by_gates_first() {
        let mut tr = t(JITA, PERIMETER);
        tr.gap_secs = 600;
        assert_eq!(verdict(&tr, &Clones::default()), Verdict::Explained(Explained::Gates(1)));
    }
}
