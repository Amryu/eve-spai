//! Whether the logi in a fleet is logi the fleet can use.
//!
//! Counting hulls in the Logistics group overstates it. A Guardian in a frigate gang cannot keep
//! up, a Scimitar cannot rep an armor fleet, and a hull the doctrine never asked for is a pilot
//! who brought the wrong ship rather than cover. Each of those is worth naming, because an FC who
//! is told "four logi, two of them useless" can do something about it.

use super::doctrine::{Category, Doctrine};
use super::model::{Composition, Member};

pub use super::doctrine::Tank;

/// How big the hulls are, which is what decides whether logi can hold the fleet's speed and range.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Size {
    Small,
    #[default]
    Line,
    Capital,
}

impl Size {
    pub fn label(self) -> &'static str {
        match self {
            Size::Small => "frigate",
            Size::Line => "cruiser",
            Size::Capital => "capital",
        }
    }
}

/// Every hull that reps a fleet, with what it reps and what it can keep up with.
///
/// By name rather than by group, because the T1 logi cruisers sit in the Cruiser group and would
/// otherwise not count at all.
const LOGI_HULLS: &[(&str, Tank, Size)] = &[
    ("Basilisk", Tank::Shield, Size::Line),
    ("Scimitar", Tank::Shield, Size::Line),
    ("Osprey", Tank::Shield, Size::Line),
    ("Scythe", Tank::Shield, Size::Line),
    ("Guardian", Tank::Armor, Size::Line),
    ("Oneiros", Tank::Armor, Size::Line),
    ("Augoror", Tank::Armor, Size::Line),
    ("Exequror", Tank::Armor, Size::Line),
    ("Kirin", Tank::Shield, Size::Small),
    ("Scalpel", Tank::Shield, Size::Small),
    ("Bantam", Tank::Shield, Size::Small),
    ("Burst", Tank::Shield, Size::Small),
    ("Deacon", Tank::Armor, Size::Small),
    ("Thalia", Tank::Armor, Size::Small),
    ("Navitas", Tank::Armor, Size::Small),
    ("Inquisitor", Tank::Armor, Size::Small),
    ("Minokawa", Tank::Shield, Size::Capital),
    ("Lif", Tank::Shield, Size::Capital),
    ("Apostle", Tank::Armor, Size::Capital),
    ("Ninazu", Tank::Armor, Size::Capital),
];

/// Ship groups that are logi whatever the hull is called, for anything the table above misses.
const LOGI_GROUPS: &[(&str, Size)] = &[
    ("Logistics", Size::Line),
    ("Logistics Frigate", Size::Small),
    ("Force Auxiliary", Size::Capital),
];

/// What a hull reps and what it keeps up with, or `None` when it is not a logi hull at all.
pub fn logi_hull(ship: &str, group: &str) -> Option<(Option<Tank>, Size)> {
    let ship = ship.trim();
    if let Some((_, tank, size)) = LOGI_HULLS.iter().find(|(n, ..)| n.eq_ignore_ascii_case(ship)) {
        return Some((Some(*tank), *size));
    }
    LOGI_GROUPS
        .iter()
        .find(|(g, _)| g.eq_ignore_ascii_case(group.trim()))
        .map(|(_, size)| (None, *size))
}

/// Why a logi pilot does not count towards cover.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reason {
    OffDoctrine,
    WrongTank,
    TooSmall,
    TooBig,
}

impl Report {
    /// The rejects, one row per hull and reason, most pilots first.
    pub fn rejected_groups(&self) -> Vec<RejectedGroup> {
        let mut out: Vec<RejectedGroup> = Vec::new();
        for r in &self.rejected {
            match out.iter_mut().find(|g| g.ship == r.ship && g.why == r.why) {
                Some(g) => g.pilots.push(r.pilot.clone()),
                None => out.push(RejectedGroup {
                    ship: r.ship.clone(),
                    why: r.why,
                    pilots: vec![r.pilot.clone()],
                }),
            }
        }
        for g in &mut out {
            g.pilots.sort();
        }
        out.sort_by(|a, b| b.pilots.len().cmp(&a.pilots.len()).then_with(|| a.ship.cmp(&b.ship)));
        out
    }
}

impl Reason {
    pub fn label(self) -> &'static str {
        match self {
            Reason::OffDoctrine => "not in the doctrine",
            Reason::WrongTank => "wrong tank",
            Reason::TooSmall => "too small for the fleet",
            Reason::TooBig => "too big for the fleet",
        }
    }
}

/// One logi pilot that was not counted.
#[derive(Clone, PartialEq, Debug)]
pub struct Rejected {
    pub pilot: String,
    pub ship: String,
    pub why: Reason,
}

/// Rejected logi grouped by hull and reason: "4 Basilisk, too big for the fleet" rather than four
/// lines naming each pilot, which in a large fleet ran the length of the sidebar.
#[derive(Clone, PartialEq, Debug)]
pub struct RejectedGroup {
    pub ship: String,
    pub why: Reason,
    /// Kept for the hover, so the FC can still find who to talk to.
    pub pilots: Vec<String>,
}

/// What the fleet's logi actually amounts to.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Report {
    /// Logi that can do the job.
    pub counted: usize,
    /// Everyone in the fleet.
    pub fleet: usize,
    /// Logi hulls that will not, and why.
    pub rejected: Vec<Rejected>,
    /// What the fleet reps with, when it can be told.
    pub tank: Option<Tank>,
    /// What size logi it needs.
    pub size: Size,
}

impl Report {
    pub fn share(&self) -> f32 {
        if self.fleet == 0 {
            0.0
        } else {
            self.counted as f32 / self.fleet as f32
        }
    }

    /// Every logi hull in the fleet, counted or not.
    pub fn brought(&self) -> usize {
        self.counted + self.rejected.len()
    }
}

/// The size of hull the fleet is built around, taken from what most of it is flying.
///
/// Logi is left out of the vote: a fleet of frigates with two Guardians in it is a frigate fleet
/// with the wrong logi, not a cruiser fleet. So is any hull whose group is unknown, which is
/// every hull when the tree came from the roster and nothing filled the groups in.
pub fn fleet_size(comp: &Composition) -> Size {
    let mut small = 0usize;
    let mut line = 0usize;
    let mut capital = 0usize;
    for m in comp.members() {
        if logi_hull(&m.ship_type_name, &m.ship_group).is_some() {
            continue;
        }
        match Category::of(&m.ship_group) {
            Category::Pod => {}
            Category::Capital => capital += 1,
            Category::Tackle | Category::Interdiction => small += 1,
            _ => match m.ship_group.trim().to_lowercase().as_str() {
                // A hull whose group we never resolved abstains. Counting it as a cruiser once
                // made every roster-only fleet ask for cruiser logi, whatever it was flying.
                "" => {}
                "frigate" | "destroyer" | "tactical destroyer" | "assault frigate"
                | "covert ops" | "stealth bomber" | "electronic attack ship" | "interceptor"
                | "expedition frigate" | "corvette" => small += 1,
                _ => line += 1,
            },
        }
    }
    if capital > small + line {
        Size::Capital
    } else if small > line {
        Size::Small
    } else {
        Size::Line
    }
}

/// What logi the doctrine asks for, read off the logi hulls it lists.
///
/// Far better evidence than counting who turned up: a Flycatcher doctrine that lists Kirin and
/// Scalpel is stating outright that its logi is frigate-sized, where a headcount is swayed by
/// boosters, support, and whoever happens to be in fleet first.
///
/// The size is the one the most listed hulls share, smallest on a tie. The tank comes with it when
/// every listed hull reps the same way, and is `None` when they disagree.
pub fn doctrine_logi(doctrine: Option<&Doctrine>) -> Option<(Option<Tank>, Size)> {
    let d = doctrine?;
    let hulls: Vec<(Option<Tank>, Size)> =
        d.ships.iter().filter_map(|s| logi_hull(&s.name, "")).collect();
    if hulls.is_empty() {
        return None;
    }
    let size = [Size::Small, Size::Line, Size::Capital]
        .into_iter()
        .map(|s| (hulls.iter().filter(|(_, hs)| *hs == s).count(), s))
        .filter(|(n, _)| *n > 0)
        .max_by_key(|(n, s)| (*n, std::cmp::Reverse(*s)))
        .map(|(_, s)| s)?;
    let tanks: Vec<Tank> = hulls.iter().filter_map(|(t, _)| *t).collect();
    let tank = tanks.first().copied().filter(|f| tanks.iter().all(|t| t == f));
    Some((tank, size))
}

/// Whether the doctrine names this exact logi hull, which settles the size question for it.
fn doctrine_names(doctrine: Option<&Doctrine>, ship: &str) -> bool {
    doctrine.is_some_and(|d| {
        d.ships.iter().any(|s| {
            s.name.trim().eq_ignore_ascii_case(ship.trim()) && logi_hull(&s.name, "").is_some()
        })
    })
}

/// Sorts the fleet's logi into what counts and what does not.
///
/// `tank` is what the fleet reps with, which the caller knows from the doctrine's boost
/// requirements. Without it nothing is rejected for the wrong tank, since guessing would call
/// perfectly good logi useless.
pub fn report(comp: &Composition, doctrine: Option<&Doctrine>, tank: Option<Tank>) -> Report {
    let from_doctrine = doctrine_logi(doctrine);
    let size = from_doctrine.map(|(_, s)| s).unwrap_or_else(|| fleet_size(comp));
    // The doctrine's own logi hulls settle the tank too, when the boosts did not say.
    let tank = tank.or_else(|| from_doctrine.and_then(|(t, _)| t));
    let mut out = Report { fleet: comp.total(), tank, size, ..Report::default() };
    for m in comp.members() {
        let Some((hull_tank, hull_size)) = logi_hull(&m.ship_type_name, &m.ship_group) else {
            continue;
        };
        match reject(comp, m, hull_tank, hull_size, size, tank, doctrine) {
            Some(why) => out.rejected.push(Rejected {
                pilot: m.name.clone(),
                ship: m.ship_type_name.clone(),
                why,
            }),
            None => out.counted += 1,
        }
    }
    out.rejected.sort_by(|a, b| a.pilot.cmp(&b.pilot));
    out
}

/// The first thing wrong with a logi hull, worst first: a ship nobody asked for is a conversation
/// before its tank is, and a hull that cannot keep up with the fleet cannot help whichever way it
/// reps.
#[allow(clippy::too_many_arguments)]
fn reject(
    comp: &Composition,
    m: &Member,
    hull_tank: Option<Tank>,
    hull_size: Size,
    want_size: Size,
    want_tank: Option<Tank>,
    doctrine: Option<&Doctrine>,
) -> Option<Reason> {
    if doctrine.is_some() && super::doctrine::classify_in(comp, m, doctrine).odd() {
        return Some(Reason::OffDoctrine);
    }
    // A doctrine that lists this hull has already answered the size question, and a doctrine
    // listing two tiers is happy with either. Only judge size for a hull it never named.
    if !doctrine_names(doctrine, &m.ship_type_name) {
        match hull_size.cmp(&want_size) {
            std::cmp::Ordering::Less => return Some(Reason::TooSmall),
            std::cmp::Ordering::Greater => return Some(Reason::TooBig),
            std::cmp::Ordering::Equal => {}
        }
    }
    match (hull_tank, want_tank) {
        (Some(a), Some(b)) if a != b => Some(Reason::WrongTank),
        _ => None,
    }
}

/// A fleet nobody could classify must not be called a cruiser fleet: that is what asked a
/// Flycatcher fleet for Guardians.
#[cfg(test)]
mod size_tests {
    use super::*;
    use crate::fleets::model::{Squad, SquadId, Wing, WingId};
    use crate::fleets::doctrine::Doctrine;

    fn comp_of(groups: &[(&str, &str)]) -> Composition {
        let members = groups
            .iter()
            .enumerate()
            .map(|(i, (ship, group))| Member {
                character_id: i as i64,
                name: format!("Pilot {i}"),
                ship_type_id: i as i64,
                ship_type_name: (*ship).to_owned(),
                ship_group: (*group).to_owned(),
                role: String::new(),
            pap_count: 0,
        })
            .collect();
        Composition {
            commander: None,
            wings: vec![Wing {
                id: WingId(1),
                name: "Wing 1".into(),
                commander: None,
                squads: vec![Squad {
                    id: SquadId(1),
                    name: "Squad 1".into(),
                    commander: None,
                    members,
                }],
            }],
            flat: false,
        }
    }

    fn doctrine_of(ships: &[&str]) -> Doctrine {
        Doctrine {
            setup_id: crate::fleets::model::SetupId(1),
            setup_name: "Test".to_owned(),
            ships: ships
                .iter()
                .map(|n| crate::fleets::doctrine::DoctrineShip { type_id: 0, name: (*n).to_owned(), main: false })
                .collect(),
            support: Vec::new(),
            tank: None,
            strict: false,
        }
    }

    /// The doctrine's own logi hulls are the answer, not a headcount of who turned up.
    #[test]
    fn the_doctrine_says_what_logi_it_wants() {
        let dictors = doctrine_of(&["Flycatcher", "Kirin", "Scalpel"]);
        assert_eq!(doctrine_logi(Some(&dictors)), Some((Some(Tank::Shield), Size::Small)));

        let cruisers = doctrine_of(&["Ferox Navy Issue", "Basilisk"]);
        assert_eq!(doctrine_logi(Some(&cruisers)), Some((Some(Tank::Shield), Size::Line)));

        // Two tiers listed: the one more hulls agree on, and no tank claim when they disagree.
        let both = doctrine_of(&["Kirin", "Scalpel", "Guardian"]);
        assert_eq!(doctrine_logi(Some(&both)), Some((None, Size::Small)));

        // A doctrine that names no logi has nothing to say, and the headcount stands.
        assert_eq!(doctrine_logi(Some(&doctrine_of(&["Flycatcher"]))), None);
        assert_eq!(doctrine_logi(None), None);
    }

    /// The bug this came from: a Flycatcher fleet read through the roster asked for Guardians and
    /// rejected the Kirins its own doctrine lists.
    #[test]
    fn dictor_doctrine_logi_is_not_rejected_for_being_small() {
        let d = doctrine_of(&["Flycatcher", "Kirin", "Scalpel"]);
        // Groups blank, the way the roster path used to leave them.
        let comp = comp_of(&[
            ("Flycatcher", ""),
            ("Flycatcher", ""),
            ("Kirin", ""),
            ("Scalpel", ""),
        ]);
        let r = report(&comp, Some(&d), None);
        assert_eq!(r.size, Size::Small, "the doctrine names frigate logi");
        assert_eq!(r.tank, Some(Tank::Shield), "and shield logi at that");
        assert_eq!(r.rejected, vec![], "its own logi must count");
        assert_eq!(r.counted, 2);

        // A hull the doctrine never named is still judged on size.
        let comp = comp_of(&[("Flycatcher", "Interdictor"), ("Guardian", "Logistics")]);
        let r = report(&comp, Some(&d), None);
        assert_eq!(r.counted, 0);
        assert_eq!(r.rejected.len(), 1);
    }

    #[test]
    fn dictors_make_a_small_fleet_and_unknown_hulls_abstain() {
        let dictors = [("Flycatcher", "Interdictor"); 10];
        assert_eq!(fleet_size(&comp_of(&dictors)), Size::Small);

        // The roster path used to hand every member an empty group, and every one of them voted
        // for cruiser.
        let unknown: Vec<(&str, &str)> =
            dictors.iter().map(|(s, _)| (*s, "")).collect();
        assert_ne!(fleet_size(&comp_of(&unknown)), Size::Small, "nothing to go on");

        // One classified dictor is enough to settle it, however many unknowns surround it.
        let mut mixed: Vec<(&str, &str)> = unknown.clone();
        mixed.push(("Flycatcher", "Interdictor"));
        assert_eq!(fleet_size(&comp_of(&mixed)), Size::Small);

        // Boosters do not drag a dictor fleet up to cruiser size.
        let mut boosted: Vec<(&str, &str)> = dictors.to_vec();
        boosted.push(("Damnation", "Command Ship"));
        assert_eq!(fleet_size(&comp_of(&boosted)), Size::Small);
    }
}

#[cfg(test)]
mod tests {
    /// A fleet with a dozen wrong-sized logi reads as one row per hull and reason, most first,
    /// with every pilot still reachable for the FC who has to talk to them.
    #[test]
    fn rejected_logi_is_summed_by_hull_and_reason() {
        let r = |pilot: &str, ship: &str, why| super::Rejected {
            pilot: pilot.to_owned(),
            ship: ship.to_owned(),
            why,
        };
        let report = super::Report {
            rejected: vec![
                r("B", "Basilisk", super::Reason::TooBig),
                r("A", "Basilisk", super::Reason::TooBig),
                r("C", "Guardian", super::Reason::WrongTank),
                r("D", "Basilisk", super::Reason::OffDoctrine),
                r("E", "Basilisk", super::Reason::TooBig),
            ],
            ..Default::default()
        };
        let g = report.rejected_groups();
        assert_eq!(g.len(), 3, "one row per hull and reason: {g:?}");
        assert_eq!(g[0].ship, "Basilisk");
        assert_eq!(g[0].why, super::Reason::TooBig);
        assert_eq!(g[0].pilots, vec!["A", "B", "E"]);
        // The same hull for a different reason is a different conversation, so its own row.
        assert!(g.iter().any(|x| x.ship == "Basilisk" && x.why == super::Reason::OffDoctrine));
        assert_eq!(g.iter().map(|x| x.pilots.len()).sum::<usize>(), 5, "a pilot went missing");
    }

    use super::*;
    use crate::fleets::doctrine::DoctrineShip;
    use crate::fleets::model::{Squad, SquadId, Wing, WingId};

    fn member(id: i64, ship: &str, group: &str) -> Member {
        Member {
            character_id: id,
            name: format!("Pilot {id}"),
            ship_type_id: id,
            ship_type_name: ship.to_owned(),
            ship_group: group.to_owned(),
            role: String::new(),
            pap_count: 0,
        }
    }

    fn comp(members: Vec<Member>) -> Composition {
        Composition {
            flat: false,
            commander: None,
            wings: vec![Wing {
                id: WingId(1),
                name: "Wing 1".into(),
                commander: None,
                squads: vec![Squad {
                    id: SquadId(1),
                    name: "Squad 1".into(),
                    commander: None,
                    members,
                }],
            }],
        }
    }

    /// T1 logi sits in the Cruiser group, so a table of hulls is what finds it.
    #[test]
    fn every_logi_hull_is_recognised() {
        assert_eq!(logi_hull("Basilisk", "Logistics"), Some((Some(Tank::Shield), Size::Line)));
        assert_eq!(logi_hull("Osprey", "Cruiser"), Some((Some(Tank::Shield), Size::Line)));
        assert_eq!(logi_hull("Guardian", "Logistics"), Some((Some(Tank::Armor), Size::Line)));
        assert_eq!(logi_hull("Kirin", "Logistics Frigate"), Some((Some(Tank::Shield), Size::Small)));
        assert_eq!(logi_hull("Apostle", "Force Auxiliary"), Some((Some(Tank::Armor), Size::Capital)));
        // Something in the group that the table has never heard of still counts, tank unknown.
        assert_eq!(logi_hull("Newhull", "Logistics"), Some((None, Size::Line)));
        assert_eq!(logi_hull("Vindicator", "Battleship"), None);
        assert_eq!(logi_hull("Osprey Navy Issue", "Cruiser"), None);
    }

    /// The fleet's size is what most of it is flying, with logi left out of the vote.
    #[test]
    fn the_fleet_size_ignores_its_own_logi() {
        let frigs = comp(vec![
            member(1, "Harpy", "Assault Frigate"),
            member(2, "Harpy", "Assault Frigate"),
            member(3, "Flycatcher", "Interdictor"),
            member(4, "Guardian", "Logistics"),
            member(5, "Guardian", "Logistics"),
        ]);
        assert_eq!(fleet_size(&frigs), Size::Small);

        let cruisers = comp(vec![
            member(1, "Muninn", "Heavy Assault Cruiser"),
            member(2, "Muninn", "Heavy Assault Cruiser"),
            member(3, "Kirin", "Logistics Frigate"),
        ]);
        assert_eq!(fleet_size(&cruisers), Size::Line);

        let caps = comp(vec![
            member(1, "Revelation", "Dreadnought"),
            member(2, "Revelation", "Dreadnought"),
            member(3, "Apostle", "Force Auxiliary"),
        ]);
        assert_eq!(fleet_size(&caps), Size::Capital);
    }

    /// Logi that cannot do the job is named rather than counted.
    #[test]
    fn logi_that_cannot_do_the_job_is_not_counted() {
        let c = comp(vec![
            member(1, "Muninn", "Heavy Assault Cruiser"),
            member(2, "Muninn", "Heavy Assault Cruiser"),
            member(3, "Muninn", "Heavy Assault Cruiser"),
            member(4, "Scimitar", "Logistics"),
            member(5, "Guardian", "Logistics"),
            member(6, "Kirin", "Logistics Frigate"),
            member(7, "Minokawa", "Force Auxiliary"),
        ]);
        let r = report(&c, None, Some(Tank::Shield));
        assert_eq!(r.counted, 1, "only the Scimitar keeps up and reps the right way");
        assert_eq!(r.brought(), 4);
        let why = |ship: &str| r.rejected.iter().find(|x| x.ship == ship).map(|x| x.why);
        assert_eq!(why("Guardian"), Some(Reason::WrongTank));
        assert_eq!(why("Kirin"), Some(Reason::TooSmall));
        assert_eq!(why("Minokawa"), Some(Reason::TooBig));

        // Size comes first: a capital that also reps the wrong way is called too big, which is
        // the thing the pilot can actually do something about.
        let armor_cap = comp(vec![
            member(1, "Muninn", "Heavy Assault Cruiser"),
            member(2, "Apostle", "Force Auxiliary"),
        ]);
        assert_eq!(report(&armor_cap, None, Some(Tank::Shield)).rejected[0].why, Reason::TooBig);
    }

    /// Without knowing how the fleet tanks, nothing is rejected for its tank.
    #[test]
    fn an_unknown_tank_rejects_nobody_for_it() {
        let c = comp(vec![
            member(1, "Muninn", "Heavy Assault Cruiser"),
            member(2, "Scimitar", "Logistics"),
            member(3, "Guardian", "Logistics"),
        ]);
        let r = report(&c, None, None);
        assert_eq!(r.counted, 2);
        assert!(r.rejected.is_empty());
    }

    /// A logi hull the doctrine never asked for is the first thing said about it.
    #[test]
    fn off_doctrine_logi_is_called_out_before_anything_else() {
        let d = Doctrine {
            setup_id: crate::fleets::model::SetupId(46),
            setup_name: "Shield Cruisers".into(),
            ships: vec![
                DoctrineShip { type_id: 1, name: "Muninn".into(), main: false },
                DoctrineShip { type_id: 4, name: "Scimitar".into(), main: false },
            ],
            support: Vec::new(),
            tank: Some(Tank::Shield),
            strict: false,
        };
        let c = comp(vec![
            member(1, "Muninn", "Heavy Assault Cruiser"),
            member(4, "Scimitar", "Logistics"),
            // Right tank, right size, but nobody asked for it.
            member(9, "Osprey", "Cruiser"),
        ]);
        let r = report(&c, Some(&d), Some(Tank::Shield));
        assert_eq!(r.counted, 1);
        assert_eq!(r.rejected.len(), 1);
        assert_eq!(r.rejected[0].ship, "Osprey");
        assert_eq!(r.rejected[0].why, Reason::OffDoctrine);
    }

    /// A fleet with no logi at all reports a share of nothing rather than dividing by zero.
    #[test]
    fn an_empty_fleet_divides_by_nothing() {
        let r = report(&comp(vec![]), None, None);
        assert_eq!(r.share(), 0.0);
        assert_eq!(r.brought(), 0);
        assert_eq!(r.fleet, 0);
    }
}
