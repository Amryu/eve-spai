//! Whether a ship belongs in the fleet it is in.
//!
//! A doctrine names the hulls the fleet is flying. Everything else is either a job that every fleet
//! needs (a cyno, a titan to bridge it, a ceptor out scouting) or something nobody asked for, and
//! only the second is worth pointing at.

use super::model::*;

/// How a hull sits with the doctrine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Standing {
    /// A hull the doctrine asks for.
    Doctrine,
    /// Not in the doctrine, but doing a job fleets need anyway.
    Support,
    /// Neither. Worth a word with the pilot.
    Unexpected,
}

impl Standing {
    pub fn label(self) -> &'static str {
        match self {
            Standing::Doctrine => "doctrine",
            Standing::Support => "support",
            Standing::Unexpected => "not in doctrine",
        }
    }

    /// Whether it is worth saying anything about.
    pub fn odd(self) -> bool {
        matches!(self, Standing::Unexpected)
    }
}

/// Ship groups that are welcome in any fleet, whatever it is flying.
///
/// By group rather than by hull, because the list is about the job: anything that bridges, lights a
/// cyno, scouts ahead or has already lost its ship is not an out-of-doctrine pilot.
pub const SUPPORT_GROUPS: &[&str] = &[
    "Titan",            // bridges the fleet
    "Force Recon Ship", // cyno
    "Interceptor",      // scouting and fast tackle
    "Interdictor",      // bubbles, wanted whatever the doctrine is
];

/// What a hull is for, which is what an FC reads a composition by: how much logi, how much tackle,
/// who can stop something leaving.
///
/// Coarser than the API's ship group, because "Logistics" and "Logistics Frigate" answer the same
/// question, and a composition listing every group separately is a list to count rather than read.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Category {
    Logistics,
    Interdiction,
    Tackle,
    Command,
    Recon,
    Capital,
    Pod,
    Line,
}

const CATEGORY_GROUPS: &[(Category, &[&str])] = &[
    (Category::Logistics, &["Logistics", "Logistics Frigate", "Force Auxiliary"]),
    (Category::Interdiction, &["Interdictor", "Heavy Interdiction Cruiser"]),
    // Assault frigates are the damage in a frigate fleet, not the tackle on it.
    (Category::Tackle, &["Interceptor", "Command Destroyer"]),
    (Category::Command, &["Command Ship"]),
    (
        Category::Recon,
        &[
            "Force Recon Ship",
            "Combat Recon Ship",
            "Covert Ops",
            "Electronic Attack Ship",
            "Stealth Bomber",
        ],
    ),
    (
        Category::Capital,
        &[
            "Titan",
            "Supercarrier",
            "Carrier",
            "Dreadnought",
            "Lancer Dreadnought",
            "Black Ops",
            "Freighter",
            "Jump Freighter",
            "Capital Industrial Ship",
        ],
    ),
    (Category::Pod, &["Capsule", "Shuttle", "Corvette"]),
];

impl Category {
    /// Anything unlisted is line: an unknown group is a hull doing the shooting until something
    /// says otherwise.
    pub fn of(group: &str) -> Category {
        let g = group.trim();
        CATEGORY_GROUPS
            .iter()
            .find(|(_, groups)| groups.iter().any(|x| x.eq_ignore_ascii_case(g)))
            .map(|(c, _)| *c)
            .unwrap_or(Category::Line)
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Logistics => "Logi",
            Category::Interdiction => "Interdiction",
            Category::Tackle => "Tackle",
            Category::Command => "Command",
            Category::Recon => "Recon",
            Category::Capital => "Capital",
            Category::Pod => "Pods",
            Category::Line => "Line",
        }
    }
}

/// Which way a hull is repaired.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tank {
    Shield,
    Armor,
}

impl Tank {
    pub fn label(self) -> &'static str {
        match self {
            Tank::Shield => "shield",
            Tank::Armor => "armor",
        }
    }

    /// Empty or unknown is "does not matter", not a guess.
    pub fn parse(s: &str) -> Option<Tank> {
        match s.trim().to_lowercase().as_str() {
            "shield" => Some(Tank::Shield),
            "armor" | "armour" => Some(Tank::Armor),
            _ => None,
        }
    }
}

/// The hulls a setup flies, plus the ones welcome in any fleet.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Doctrine {
    pub setup_id: SetupId,
    pub setup_name: String,
    pub ships: Vec<DoctrineShip>,
    /// Hulls welcome in any fleet, carried here so one lookup answers the whole question.
    pub support: Vec<DoctrineShip>,
    /// How this doctrine tanks, when it can be told.
    pub tank: Option<Tank>,
    /// Takes its own hulls and nothing else. An entosis op or a covert one is restricted enough
    /// that a bridging titan parked in it is still the wrong ship.
    pub strict: bool,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct DoctrineShip {
    /// 0 when the hull is known by name only, which is how the user configures it.
    pub type_id: i64,
    pub name: String,
    /// Marked as what the fleet is built around, so its role is damage whatever its hull group
    /// would otherwise say.
    pub main: bool,
}

impl Doctrine {
    pub fn has(&self, type_id: i64) -> bool {
        self.ships.iter().any(|s| s.type_id != 0 && s.type_id == type_id)
    }

    /// By type id where there is one, since the client renders hull names in the player's own
    /// language and the id is the same everywhere.
    fn found<'a>(list: &'a [DoctrineShip], type_id: i64, ship: &str) -> Option<&'a DoctrineShip> {
        let ship = ship.trim();
        list.iter().find(|s| {
            (s.type_id != 0 && s.type_id == type_id) || s.name.trim().eq_ignore_ascii_case(ship)
        })
    }

    /// Hulls the doctrine wants that nobody is flying.
    pub fn missing(&self, comp: &Composition) -> Vec<String> {
        self.ships
            .iter()
            .filter(|s| {
                !comp.members().any(|m| {
                    (s.type_id != 0 && m.ship_type_id == s.type_id)
                        || m.ship_type_name.trim().eq_ignore_ascii_case(s.name.trim())
                })
            })
            .map(|s| s.name.clone())
            .collect()
    }
}

/// The setup that means "no setup": the FC brings whatever they like, so there is nothing to be
/// out of. Held by id rather than by name, since the name is alliance-side and could change.
pub const FC_CHOICE: SetupId = SetupId(61);

/// Whether a setup describes a doctrine at all.
pub fn is_doctrine(setup_id: SetupId) -> bool {
    setup_id != FC_CHOICE && setup_id.0 != 0
}

/// Builds a doctrine from what the user configured, folding in whatever the seed knows.
///
/// `hulls` carries every configured hull for every setup; the ones with setup 0 are welcome in any
/// fleet. `tank` is what the doctrine's boosts say it reps with.
pub fn configured(
    setup_id: SetupId,
    setup_name: &str,
    seeded: Option<Doctrine>,
    hulls: &[crate::settings::FleetHull],
    tank: Option<Tank>,
    strict: bool,
) -> Option<Doctrine> {
    if !is_doctrine(setup_id) {
        return None;
    }
    let ship = |h: &crate::settings::FleetHull| DoctrineShip {
        type_id: h.type_id,
        name: h.name.trim().to_owned(),
        main: h.main,
    };
    let mut ships: Vec<DoctrineShip> = seeded.as_ref().map(|d| d.ships.clone()).unwrap_or_default();
    for h in hulls.iter().filter(|h| h.setup_id == setup_id.0 && !h.name.trim().is_empty()) {
        if !ships.iter().any(|s| s.name.trim().eq_ignore_ascii_case(h.name.trim())) {
            ships.push(ship(h));
        }
    }
    let support: Vec<DoctrineShip> =
        hulls.iter().filter(|h| h.setup_id == 0 && !h.name.trim().is_empty()).map(ship).collect();
    // Nothing configured and nothing seeded is no doctrine at all, which is what stops every hull
    // in the fleet reading as out of doctrine. A strict one still needs its own hulls.
    if ships.is_empty() && (strict || support.is_empty()) {
        return None;
    }
    Some(Doctrine {
        setup_id,
        setup_name: seeded
            .map(|d| d.setup_name)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| setup_name.to_owned()),
        ships,
        support: if strict { Vec::new() } else { support },
        tank,
        strict,
    })
}

/// Where a hull stands.
///
/// A configured doctrine list is the answer when there is one. Without one, only the jobs every
/// fleet needs are waved through: silence is not a reason to accept a ship, but it is also why the
/// list has to be configurable, since the dashboard hands over no hulls at all.
pub fn classify(type_id: i64, ship: &str, group: &str, doctrine: Option<&Doctrine>) -> Standing {
    let Some(d) = doctrine else {
        return if is_support_group(group) { Standing::Support } else { Standing::Unexpected };
    };
    if Doctrine::found(&d.ships, type_id, ship).is_some() {
        return Standing::Doctrine;
    }
    if d.strict {
        // Nothing is waved through: the whole point of the flag is that this fleet takes its own
        // hulls and no others.
        return Standing::Unexpected;
    }
    if Doctrine::found(&d.support, type_id, ship).is_some() {
        return Standing::Support;
    }
    if is_support_group(group) {
        return Standing::Support;
    }
    Standing::Unexpected
}

/// Hulls that are in fleet to move it, not to fight in it.
///
/// A bridge is dropped into a fleet for one cyno and is meant to look out of place, so it survives
/// a sweep of everything that does not belong even where the doctrine refuses everything else.
pub const BRIDGE_GROUPS: &[&str] = &["Titan", "Black Ops"];

/// Pilots the FC has confirmed are meant to be in what they are in.
pub type Locked = std::collections::BTreeSet<i64>;

/// Who a sweep of the off-doctrine pilots would remove.
///
/// Four exemptions. Anybody the FC locked, which is the whole point of locking. Anybody holding a
/// command seat, because kicking a wing commander out of the fleet they are running is never the
/// intent. Bridges, dropped in for one cyno and meant to look out of place. And the fleet boss,
/// already exempt through `classify_in`.
pub fn off_doctrine_kickable<'a>(
    comp: &'a Composition,
    doctrine: Option<&Doctrine>,
    locked: &Locked,
) -> Vec<&'a Member> {
    comp.members()
        .filter(|m| classify_in(comp, m, doctrine).odd())
        .filter(|m| !locked.contains(&m.character_id))
        .filter(|m| matches!(comp.seat_of(m.character_id), Some(Seat::Squad(..))))
        .filter(|m| !BRIDGE_GROUPS.iter().any(|g| g.eq_ignore_ascii_case(m.ship_group.trim())))
        .collect()
}

/// Where a hull stands, for a pilot in a known fleet.
///
/// The fleet boss is never out of doctrine: the FC flies whatever the job needs, and telling them
/// their own ship is wrong is the one thing this list must not do.
pub fn classify_in(comp: &Composition, m: &Member, doctrine: Option<&Doctrine>) -> Standing {
    if comp.commander.as_ref().is_some_and(|c| c.character_id == m.character_id) {
        return Standing::Doctrine;
    }
    classify(m.ship_type_id, &m.ship_type_name, &m.ship_group, doctrine)
}

fn is_support_group(group: &str) -> bool {
    SUPPORT_GROUPS.iter().any(|g| g.eq_ignore_ascii_case(group.trim()))
}

/// One hull in the fleet, with how many are flying it.
#[derive(Clone, PartialEq, Debug)]
pub struct ShipLine {
    pub type_id: i64,
    pub name: String,
    pub group: String,
    pub category: Category,
    pub count: usize,
    pub standing: Standing,
}

/// What a doctrine hull is in the fleet to do.
///
/// A doctrine list is flat, but an FC reads it as four questions: is the damage there, can it be
/// repped, is it boosted, and is the support that lets it engage present.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Role {
    Dps,
    Logi,
    Boosts,
    Support,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Dps, Role::Logi, Role::Boosts, Role::Support];

    pub fn label(self) -> &'static str {
        match self {
            Role::Dps => "DPS",
            Role::Logi => "Logi",
            Role::Boosts => "Boosts",
            Role::Support => "Support",
        }
    }

    /// Command destroyers are the awkward one: boosh in a fleet that boosts off command ships,
    /// boosters in one that does not, which is the same rule `checks::boosters` applies.
    ///
    /// A hull the doctrine marks as its main wins over all of it. Nothing about an Interdictor
    /// says "this is the damage", but in a Flycatcher fleet it is.
    pub fn of(line: &ShipLine, doctrine: Option<&Doctrine>, command_ships: bool) -> Role {
        if doctrine.is_some_and(|d| {
            d.ships.iter().any(|s| {
                s.main
                    && ((s.type_id != 0 && s.type_id == line.type_id)
                        || s.name.trim().eq_ignore_ascii_case(line.name.trim()))
            })
        }) {
            return Role::Dps;
        }
        if super::logi::logi_hull(&line.name, &line.group).is_some() {
            return Role::Logi;
        }
        match line.category {
            Category::Logistics => Role::Logi,
            Category::Command => Role::Boosts,
            Category::Interdiction | Category::Recon => Role::Support,
            Category::Tackle => {
                if line.group.trim().eq_ignore_ascii_case("Command Destroyer") && !command_ships {
                    Role::Boosts
                } else {
                    Role::Support
                }
            }
            Category::Pod => Role::Support,
            Category::Capital | Category::Line => Role::Dps,
        }
    }
}

/// The fleet grouped by hull, most flown first, so a composition reads as ships rather than names.
pub fn by_ship(comp: &Composition, doctrine: Option<&Doctrine>) -> Vec<ShipLine> {
    let mut by: std::collections::HashMap<i64, ShipLine> = Default::default();
    for m in comp.members() {
        by.entry(m.ship_type_id)
            .and_modify(|l| l.count += 1)
            .or_insert_with(|| ShipLine {
                type_id: m.ship_type_id,
                name: m.ship_type_name.clone(),
                group: m.ship_group.clone(),
                category: Category::of(&m.ship_group),
                count: 1,
                standing: classify_in(comp, m, doctrine),
            });
    }
    let mut lines: Vec<ShipLine> = by.into_values().collect();
    lines.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    lines
}

/// How many pilots are flying something nobody asked for.
pub fn unexpected_pilots(lines: &[ShipLine]) -> usize {
    lines.iter().filter(|l| l.standing.odd()).map(|l| l.count).sum()
}

/// A pilot who has been in a hull the doctrine never asked for, and since when.
#[derive(Clone, PartialEq, Debug)]
pub struct OffDoctrine {
    pub character_id: i64,
    pub name: String,
    pub ship: String,
    /// When this pilot was first seen in this hull.
    pub since: i64,
}

/// Carries forward how long each off-doctrine pilot has been in the hull they are in.
///
/// The clock belongs to the pilot and the hull together: reshipping into a different wrong ship
/// starts again, because that is a new thing to ask them about, and reshipping into the doctrine
/// drops them off the list.
pub fn track_off_doctrine(
    prev: &[OffDoctrine],
    comp: &Composition,
    doctrine: Option<&Doctrine>,
    now: i64,
) -> Vec<OffDoctrine> {
    let mut out: Vec<OffDoctrine> = comp
        .members()
        .filter(|m| classify_in(comp, m, doctrine).odd())
        .map(|m| {
            let since = prev
                .iter()
                .find(|p| p.character_id == m.character_id && p.ship == m.ship_type_name)
                .map(|p| p.since)
                .unwrap_or(now);
            OffDoctrine {
                character_id: m.character_id,
                name: m.name.clone(),
                ship: m.ship_type_name.clone(),
                since,
            }
        })
        .collect();
    out.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Long enough in the wrong ship to be worth a word, rather than someone who just undocked.
pub const OFF_DOCTRINE_GRACE: i64 = 600;

/// Who has been off doctrine longer than `grace`, longest first.
pub fn lingering(rows: &[OffDoctrine], now: i64, grace: i64) -> Vec<&OffDoctrine> {
    rows.iter().filter(|r| now - r.since >= grace).collect()
}

/// A doctrine reads as four questions, and the awkward hull is the command destroyer.
#[cfg(test)]
mod role_tests {
    use super::*;

    fn line(name: &str, group: &str) -> ShipLine {
        ShipLine {
            type_id: 1,
            name: name.to_owned(),
            group: group.to_owned(),
            category: Category::of(group),
            count: 1,
            standing: Standing::Doctrine,
        }
    }

    #[test]
    fn every_doctrine_hull_lands_in_one_of_the_four() {
        let cases = [
            ("Flycatcher", "Interdictor", Role::Support),
            ("Kirin", "Logistics Frigate", Role::Logi),
            ("Guardian", "Logistics", Role::Logi),
            ("Damnation", "Command Ship", Role::Boosts),
            ("Keres", "Electronic Attack Ship", Role::Support),
            ("Ferox Navy Issue", "Combat Battlecruiser", Role::Dps),
            ("Muninn", "Heavy Assault Cruiser", Role::Dps),
            ("Crusader", "Interceptor", Role::Support),
        ];
        for (ship, group, want) in cases {
            assert_eq!(Role::of(&line(ship, group), None, false), want, "{ship} [{group}]");
        }
    }

    /// The same hull, two answers: boosting a fleet that has no command ships, booshing one that
    /// does. Same rule the boost check applies, so the two cannot disagree.
    #[test]
    fn a_command_destroyer_follows_the_fleet_it_is_in() {
        let bifrost = line("Bifrost", "Command Destroyer");
        assert_eq!(Role::of(&bifrost, None, false), Role::Boosts, "no command ships to boost off");
        assert_eq!(Role::of(&bifrost, None, true), Role::Support, "command ships are boosting");
    }

    /// The reason this exists: a Flycatcher is an Interdictor, which reads as support in every
    /// fleet except the one built around it.
    #[test]
    fn a_hull_marked_main_is_the_damage_whatever_its_group_says() {
        let d = Doctrine {
            setup_id: crate::fleets::model::SetupId(46),
            setup_name: "Flycatchers".to_owned(),
            ships: vec![
                DoctrineShip { type_id: 22_464, name: "Flycatcher".to_owned(), main: true },
                DoctrineShip { type_id: 37_458, name: "Kirin".to_owned(), main: false },
            ],
            support: Vec::new(),
            tank: None,
            strict: false,
        };
        let fly = line("Flycatcher", "Interdictor");
        assert_eq!(Role::of(&fly, None, false), Role::Support, "without the mark it is support");
        assert_eq!(Role::of(&fly, Some(&d), false), Role::Dps, "marked, it is the damage");
        // Marking one hull does not promote the rest of the doctrine.
        assert_eq!(Role::of(&line("Kirin", "Logistics Frigate"), Some(&d), false), Role::Logi);
        // A Sabre is an interdictor the doctrine never named, so it stays support.
        assert_eq!(Role::of(&line("Sabre", "Interdictor"), Some(&d), false), Role::Support);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: i64, name: &str, group: &str) -> Member {
        Member {
            character_id: id,
            name: format!("Pilot {id}"),
            ship_type_id: id,
            ship_type_name: name.to_owned(),
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

    fn doctrine() -> Doctrine {
        Doctrine {
            setup_id: SetupId(46),
            setup_name: "Fast Tackle".into(),
            ships: vec![
                DoctrineShip { type_id: 1, name: "Flycatcher".into(), main: false },
                DoctrineShip { type_id: 2, name: "Kirin".into(), main: false },
            ],
            support: Vec::new(),
            tank: None,
            strict: false,
        }
    }

    /// The three answers, and the one that matters is the third.
    #[test]
    fn a_hull_is_doctrine_support_or_unwelcome() {
        let d = doctrine();
        assert_eq!(classify(1, "Hull", "Interdictor", Some(&d)), Standing::Doctrine);
        assert_eq!(classify(9, "Hull", "Titan", Some(&d)), Standing::Support);
        assert_eq!(classify(9, "Hull", "Battleship", Some(&d)), Standing::Unexpected);
    }

    /// The jobs every fleet needs are not out-of-doctrine pilots, whatever the doctrine is.
    #[test]
    fn the_support_jobs_are_never_flagged() {
        let d = doctrine();
        for group in ["Titan", "Force Recon Ship", "Interceptor", "Interdictor"] {
            assert_eq!(classify(99, "Hull", group, Some(&d)), Standing::Support, "{group} was flagged");
        }
        // Anything else is a choice the pilot made, and belongs in the configurable list if the
        // fleet wants it. A pod is on the list too: somebody in one has a ship to get back into.
        for group in
            ["Black Ops", "Covert Ops", "Combat Recon Ship", "Command Destroyer", "Capsule",
             "Shuttle"]
        {
            assert_eq!(
                classify(99, "Hull", group, Some(&d)),
                Standing::Unexpected,
                "{group} was waved through"
            );
        }
        // Case and stray spaces come from data, not from the pilot.
        assert_eq!(classify(99, "Hull", " force recon ship ", Some(&d)), Standing::Support);
    }

    /// The FC flies whatever the job needs, so their own ship is never the wrong one.
    #[test]
    fn the_fleet_boss_is_never_out_of_doctrine() {
        let d = doctrine();
        let boss = member(9, "Vindicator", "Battleship");
        let other = member(9, "Vindicator", "Battleship");
        let mut c = comp(vec![other.clone()]);
        assert_eq!(classify_in(&c, &other, Some(&d)), Standing::Unexpected);

        c.commander = Some(boss.clone());
        assert_eq!(classify_in(&c, &boss, Some(&d)), Standing::Doctrine);
        // Only the boss: a wing commander in the same hull is still in the wrong hull.
        let wc = member(11, "Vindicator", "Battleship");
        assert_eq!(classify_in(&c, &wc, Some(&d)), Standing::Unexpected);
    }

    /// A doctrine hull stays doctrine even when its group is also a support one, since the fleet
    /// asked for it.
    #[test]
    fn the_doctrine_wins_over_the_support_list() {
        let d = Doctrine {
            ships: vec![DoctrineShip { type_id: 5, name: "Crow".into(), main: false }],
            ..doctrine()
        };
        assert_eq!(classify(5, "Hull", "Interceptor", Some(&d)), Standing::Doctrine);
    }

    /// Without a doctrine nothing can be called out of it: only the support list applies.
    #[test]
    fn no_doctrine_means_nothing_is_out_of_it() {
        assert_eq!(classify(1, "Hull", "Battleship", None), Standing::Unexpected);
        assert_eq!(classify(1, "Hull", "Titan", None), Standing::Support);
    }

    /// The composition reads as hulls, most flown first.
    #[test]
    fn the_composition_groups_by_hull() {
        let c = comp(vec![
            member(1, "Flycatcher", "Interdictor"),
            member(1, "Flycatcher", "Interdictor"),
            member(2, "Kirin", "Logistics Frigate"),
            member(3, "Vindicator", "Battleship"),
        ]);
        let lines = by_ship(&c, Some(&doctrine()));
        assert_eq!(lines.len(), 3);
        assert_eq!((lines[0].name.as_str(), lines[0].count), ("Flycatcher", 2));
        assert_eq!(lines[0].standing, Standing::Doctrine);
        let odd = lines.iter().find(|l| l.name == "Vindicator").expect("the odd one out");
        assert_eq!(odd.standing, Standing::Unexpected);
        assert_eq!(unexpected_pilots(&lines), 1);
    }

    /// What the doctrine wants and nobody brought.
    #[test]
    fn a_doctrine_reports_what_is_missing() {
        let c = comp(vec![member(1, "Flycatcher", "Interdictor")]);
        assert_eq!(doctrine().missing(&c), vec!["Kirin".to_owned()]);
        let full = comp(vec![member(1, "F", "Interdictor"), member(2, "K", "Logistics Frigate")]);
        assert!(doctrine().missing(&full).is_empty());
    }

    /// The roles an FC reads a composition by, and everything else shooting things.
    #[test]
    fn a_hull_group_lands_in_its_role() {
        for (group, want) in [
            ("Logistics Frigate", Category::Logistics),
            ("Force Auxiliary", Category::Logistics),
            ("Heavy Interdiction Cruiser", Category::Interdiction),
            ("Command Destroyer", Category::Tackle),
            ("Assault Frigate", Category::Line),
            ("Command Ship", Category::Command),
            ("Force Recon Ship", Category::Recon),
            ("Titan", Category::Capital),
            ("Capsule", Category::Pod),
            ("Battleship", Category::Line),
            ("Heavy Assault Cruiser", Category::Line),
            (" logistics ", Category::Logistics),
            ("", Category::Line),
        ] {
            assert_eq!(Category::of(group), want, "{group}");
        }
    }

    /// The clock starts when a pilot first turns up in the wrong hull and survives the next poll.
    #[test]
    fn an_off_doctrine_pilot_keeps_their_clock() {
        let d = doctrine();
        let wrong = comp(vec![member(9, "Vindicator", "Battleship")]);
        let first = track_off_doctrine(&[], &wrong, Some(&d), 1000);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].since, 1000);

        // Still there two minutes later: the clock is the first sighting, not this one.
        let again = track_off_doctrine(&first, &wrong, Some(&d), 1120);
        assert_eq!(again[0].since, 1000);

        // Reshipping into another wrong hull is a new thing to ask about.
        let other = comp(vec![member(9, "Rokh", "Battleship")]);
        assert_eq!(track_off_doctrine(&again, &other, Some(&d), 1200)[0].since, 1200);

        // And reshipping into the doctrine takes them off the list.
        let right = comp(vec![member(1, "Flycatcher", "Interdictor")]);
        assert!(track_off_doctrine(&again, &right, Some(&d), 1300).is_empty());
    }

    /// Someone who just undocked wrong is not yet worth a report.
    #[test]
    fn only_a_long_stay_off_doctrine_is_reported() {
        let rows = vec![
            OffDoctrine { character_id: 1, name: "Late Arrival".into(), ship: "Rokh".into(), since: 1000 },
            OffDoctrine { character_id: 2, name: "Old Hand".into(), ship: "Raven".into(), since: 100 },
        ];
        let late = lingering(&rows, 1100, OFF_DOCTRINE_GRACE);
        assert_eq!(late.len(), 1);
        assert_eq!(late[0].name, "Old Hand");
        assert!(lingering(&rows, 1000, OFF_DOCTRINE_GRACE).len() <= 1);
        assert!(lingering(&rows, 100, OFF_DOCTRINE_GRACE).is_empty());
    }

    /// A configured hull list is what makes a doctrine. How it tanks belongs to the doctrine, not
    /// to each hull in it.
    #[test]
    fn the_configured_hulls_decide_what_belongs() {
        let hull = |setup: i32, name: &str| crate::settings::FleetHull { setup_id: setup, type_id: 0, name: name.to_owned(), main: false };
        let hulls =
            vec![hull(46, "Muninn"), hull(46, "Scimitar"), hull(0, "Falcon"), hull(0, "Guardian")];
        let d = configured(SetupId(46), "Shield Cruisers", None, &hulls, Some(Tank::Shield), false)
            .expect("a doctrine");
        assert_eq!(d.ships.len(), 2);
        assert_eq!(d.support.len(), 2);
        assert_eq!(d.tank, Some(Tank::Shield));

        assert_eq!(classify(0, "Muninn", "Heavy Assault Cruiser", Some(&d)), Standing::Doctrine);
        assert_eq!(classify(0, "Falcon", "Force Recon Ship", Some(&d)), Standing::Support);
        assert_eq!(classify(0, "Guardian", "Logistics", Some(&d)), Standing::Support);
        assert_eq!(classify(0, "Vindicator", "Battleship", Some(&d)), Standing::Unexpected);
        // A group the whole game needs is still waved through.
        assert_eq!(classify(0, "Erebus", "Titan", Some(&d)), Standing::Support);

        // Nothing configured and nothing seeded is no doctrine, so nothing reads as out of one.
        assert!(configured(SetupId(46), "Shield Cruisers", None, &[], None, false).is_none());
    }

    /// A restricted fleet takes its own hulls and nothing else, not even a cyno.
    #[test]
    fn a_strict_doctrine_waves_nothing_through() {
        let hull = |setup: i32, name: &str| crate::settings::FleetHull { setup_id: setup, type_id: 0, name: name.to_owned(), main: false };
        let hulls = vec![hull(19, "Hecate"), hull(0, "Falcon")];
        let d = configured(SetupId(19), "Entosis", None, &hulls, None, true).expect("a doctrine");
        assert!(d.strict);
        assert!(d.support.is_empty(), "a strict doctrine keeps no support list");

        assert_eq!(classify(0, "Hecate", "Tactical Destroyer", Some(&d)), Standing::Doctrine);
        // Configured as always allowed, and a built-in group, both refused.
        assert_eq!(classify(0, "Falcon", "Force Recon Ship", Some(&d)), Standing::Unexpected);
        assert_eq!(classify(0, "Erebus", "Titan", Some(&d)), Standing::Unexpected);

        // Strict with no hulls of its own is not a doctrine, or it would flag the whole fleet.
        assert!(configured(SetupId(19), "Entosis", None, &[hull(0, "Falcon")], None, true)
            .is_none());
    }

    /// The seed's hulls and the configured ones are one list, without repeats.
    #[test]
    fn seeded_and_configured_hulls_merge() {
        let seeded = Doctrine {
            setup_id: SetupId(46),
            setup_name: "Fast Tackle".into(),
            ships: vec![DoctrineShip { type_id: 22_464, name: "Flycatcher".into(), main: false }],
            support: Vec::new(),
            tank: None,
            strict: false,
        };
        let hulls = vec![
            crate::settings::FleetHull { setup_id: 46, type_id: 0, name: " flycatcher ".to_owned(), main: false },
            crate::settings::FleetHull { setup_id: 46, type_id: 0, name: "Harpy".to_owned(), main: false },
        ];
        let d = configured(SetupId(46), "", Some(seeded), &hulls, None, false).expect("a doctrine");
        assert_eq!(d.ships.len(), 2, "the same hull twice is one hull");
        assert_eq!(d.setup_name, "Fast Tackle");
        assert!(d.has(22_464));
        assert_eq!(classify(0, "Harpy", "Assault Frigate", Some(&d)), Standing::Doctrine);
    }

    /// FC Choice is the absence of a doctrine, so nothing can be out of it.
    #[test]
    fn fc_choice_is_not_a_doctrine() {
        assert!(!is_doctrine(FC_CHOICE));
        assert!(!is_doctrine(SetupId(0)));
        assert!(is_doctrine(SetupId(46)));

        let hulls = vec![crate::settings::FleetHull { setup_id: FC_CHOICE.0, type_id: 0, name: "Muninn".to_owned(), main: false }];
        assert!(configured(FC_CHOICE, "FC Choice", None, &hulls, None, false).is_none());
    }

    /// A sweep of the off-doctrine pilots leaves the bridges and the FC where they are.
    #[test]
    fn a_sweep_spares_the_bridges_and_the_boss() {
        let d = doctrine();
        let mut c = comp(vec![
            member(1, "Flycatcher", "Interdictor"),
            member(7, "Vindicator", "Battleship"),
            member(8, "Rokh", "Battleship"),
            member(9, "Erebus", "Titan"),
            member(10, "Sin", "Black Ops"),
        ]);
        c.commander = Some(member(11, "Raven", "Battleship"));
        c.wings[0].commander = Some(member(12, "Scorpion", "Battleship"));
        c.wings[0].squads[0].commander = Some(member(13, "Armageddon", "Battleship"));

        let none = Locked::new();
        let out: Vec<&str> = off_doctrine_kickable(&c, Some(&d), &none)
            .iter()
            .map(|m| m.ship_type_name.as_str())
            .collect();
        assert_eq!(out, vec!["Vindicator", "Rokh"], "a commander was swept");

        // A locked pilot is a confirmed one, whatever they are flying.
        let locked: Locked = [7].into_iter().collect();
        let out: Vec<&str> = off_doctrine_kickable(&c, Some(&d), &locked)
            .iter()
            .map(|m| m.ship_type_name.as_str())
            .collect();
        assert_eq!(out, vec!["Rokh"]);

        // A strict doctrine refuses the titan as a hull, and still does not sweep it.
        let strict = Doctrine { strict: true, ..d };
        let out: Vec<&str> = off_doctrine_kickable(&c, Some(&strict), &none)
            .iter()
            .map(|m| m.ship_type_name.as_str())
            .collect();
        assert!(!out.contains(&"Erebus"), "{out:?}");
        assert!(!out.contains(&"Sin"), "{out:?}");
        assert!(!out.contains(&"Raven"), "the boss was swept");
        assert!(!out.contains(&"Scorpion"), "a wing commander was swept");
        assert!(!out.contains(&"Armageddon"), "a squad commander was swept");
    }

    /// An unknown group is not a free pass.
    #[test]
    fn an_unknown_group_is_not_support() {
        assert_eq!(classify(9, "Hull", "", Some(&doctrine())), Standing::Unexpected);
    }
}
