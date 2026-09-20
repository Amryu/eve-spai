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
}

/// Ship groups that are welcome in any fleet, whatever it is flying.
///
/// By group rather than by hull, because the list is about the job: anything that bridges, lights a
/// cyno, scouts ahead or has already lost its ship is not an out-of-doctrine pilot.
pub const SUPPORT_GROUPS: &[&str] = &[
    "Titan",             // bridges the fleet
    "Black Ops",         // the same, quietly
    "Force Recon Ship",  // cyno
    "Combat Recon Ship",
    "Covert Ops",        // scouting
    "Interceptor",       // scouting and fast tackle
    "Command Destroyer", // boosh
    "Capsule",           // already lost the ship it came in
    "Shuttle",
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
    (Category::Tackle, &["Interceptor", "Assault Frigate", "Command Destroyer"]),
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

/// The hulls a setup flies.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Doctrine {
    pub setup_id: SetupId,
    pub setup_name: String,
    pub ships: Vec<DoctrineShip>,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct DoctrineShip {
    pub type_id: i64,
    pub name: String,
}

impl Doctrine {
    pub fn has(&self, type_id: i64) -> bool {
        self.ships.iter().any(|s| s.type_id == type_id)
    }

    /// Hulls the doctrine wants that nobody is flying.
    pub fn missing(&self, comp: &Composition) -> Vec<String> {
        self.ships
            .iter()
            .filter(|s| !comp.wings.iter().flat_map(|w| &w.squads).any(|sq| {
                sq.members.iter().any(|m| m.ship_type_id == s.type_id)
            }))
            .map(|s| s.name.clone())
            .collect()
    }
}

/// Where a hull stands. An unknown group counts as unexpected rather than support: silence is not
/// a reason to wave a ship through.
pub fn classify(type_id: i64, group: &str, doctrine: Option<&Doctrine>) -> Standing {
    if doctrine.is_some_and(|d| d.has(type_id)) {
        return Standing::Doctrine;
    }
    if SUPPORT_GROUPS.iter().any(|g| g.eq_ignore_ascii_case(group.trim())) {
        return Standing::Support;
    }
    Standing::Unexpected
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

/// The fleet grouped by hull, most flown first, so a composition reads as ships rather than names.
pub fn by_ship(comp: &Composition, doctrine: Option<&Doctrine>) -> Vec<ShipLine> {
    let mut by: std::collections::HashMap<i64, ShipLine> = Default::default();
    for m in comp.wings.iter().flat_map(|w| &w.squads).flat_map(|s| &s.members) {
        by.entry(m.ship_type_id)
            .and_modify(|l| l.count += 1)
            .or_insert_with(|| ShipLine {
                type_id: m.ship_type_id,
                name: m.ship_type_name.clone(),
                group: m.ship_group.clone(),
                category: Category::of(&m.ship_group),
                count: 1,
                standing: classify(m.ship_type_id, &m.ship_group, doctrine),
            });
    }
    let mut lines: Vec<ShipLine> = by.into_values().collect();
    lines.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    lines
}

/// How many pilots are flying something nobody asked for.
pub fn unexpected_pilots(lines: &[ShipLine]) -> usize {
    lines.iter().filter(|l| l.standing == Standing::Unexpected).map(|l| l.count).sum()
}

/// One role, how many are in it, and which hulls make it up.
#[derive(Clone, PartialEq, Debug)]
pub struct CategoryLine {
    pub category: Category,
    pub count: usize,
    /// Hull and how many, most flown first.
    pub ships: Vec<(String, usize)>,
}

/// The same hulls rolled up by what they are for, biggest role first.
pub fn by_category(lines: &[ShipLine]) -> Vec<CategoryLine> {
    let mut by: std::collections::BTreeMap<Category, CategoryLine> = Default::default();
    for l in lines {
        let e = by.entry(l.category).or_insert_with(|| CategoryLine {
            category: l.category,
            count: 0,
            ships: Vec::new(),
        });
        e.count += l.count;
        e.ships.push((l.name.clone(), l.count));
    }
    let mut out: Vec<CategoryLine> = by.into_values().collect();
    for c in &mut out {
        c.ships.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.category.cmp(&b.category)));
    out
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
        .wings
        .iter()
        .flat_map(|w| &w.squads)
        .flat_map(|s| &s.members)
        .filter(|m| classify(m.ship_type_id, &m.ship_group, doctrine) == Standing::Unexpected)
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
        }
    }

    fn comp(members: Vec<Member>) -> Composition {
        Composition {
            wings: vec![Wing {
                id: WingId(1),
                name: "Wing 1".into(),
                squads: vec![Squad { id: SquadId(1), name: "Squad 1".into(), members }],
            }],
        }
    }

    fn doctrine() -> Doctrine {
        Doctrine {
            setup_id: SetupId(46),
            setup_name: "Fast Tackle".into(),
            ships: vec![
                DoctrineShip { type_id: 1, name: "Flycatcher".into() },
                DoctrineShip { type_id: 2, name: "Kirin".into() },
            ],
        }
    }

    /// The three answers, and the one that matters is the third.
    #[test]
    fn a_hull_is_doctrine_support_or_unwelcome() {
        let d = doctrine();
        assert_eq!(classify(1, "Interdictor", Some(&d)), Standing::Doctrine);
        assert_eq!(classify(9, "Titan", Some(&d)), Standing::Support);
        assert_eq!(classify(9, "Battleship", Some(&d)), Standing::Unexpected);
    }

    /// The jobs every fleet needs are not out-of-doctrine pilots, whatever the doctrine is.
    #[test]
    fn the_support_jobs_are_never_flagged() {
        let d = doctrine();
        for group in
            ["Titan", "Black Ops", "Force Recon Ship", "Covert Ops", "Interceptor",
             "Command Destroyer", "Capsule", "Shuttle"]
        {
            assert_eq!(classify(99, group, Some(&d)), Standing::Support, "{group} was flagged");
        }
        // Case and stray spaces come from data, not from the pilot.
        assert_eq!(classify(99, " force recon ship ", Some(&d)), Standing::Support);
    }

    /// A doctrine hull stays doctrine even when its group is also a support one, since the fleet
    /// asked for it.
    #[test]
    fn the_doctrine_wins_over_the_support_list() {
        let d = Doctrine {
            ships: vec![DoctrineShip { type_id: 5, name: "Crow".into() }],
            ..doctrine()
        };
        assert_eq!(classify(5, "Interceptor", Some(&d)), Standing::Doctrine);
    }

    /// Without a doctrine nothing can be called out of it: only the support list applies.
    #[test]
    fn no_doctrine_means_nothing_is_out_of_it() {
        assert_eq!(classify(1, "Interdictor", None), Standing::Unexpected);
        assert_eq!(classify(1, "Titan", None), Standing::Support);
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

    /// Rolled up by role, biggest first, with the hulls behind each one.
    #[test]
    fn the_composition_rolls_up_by_role() {
        let c = comp(vec![
            member(1, "Kirin", "Logistics Frigate"),
            member(1, "Kirin", "Logistics Frigate"),
            member(2, "Scythe", "Logistics"),
            member(3, "Crow", "Interceptor"),
        ]);
        let cats = by_category(&by_ship(&c, None));
        assert_eq!(cats[0].category, Category::Logistics);
        assert_eq!(cats[0].count, 3);
        assert_eq!(cats[0].ships, vec![("Kirin".to_owned(), 2), ("Scythe".to_owned(), 1)]);
        assert_eq!(cats[1].category, Category::Tackle);
        assert_eq!(cats[1].count, 1);
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

    /// An unknown group is not a free pass.
    #[test]
    fn an_unknown_group_is_not_support() {
        assert_eq!(classify(9, "", Some(&doctrine())), Standing::Unexpected);
    }
}
