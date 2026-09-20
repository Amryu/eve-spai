//! What is thin about a fleet's composition.
//!
//! Advice, never a block: the FC decides whether to undock with four logi. Every check says what it
//! looked at and what it would rather see, so a warning can be read and dismissed in one glance.

use super::boosts::{self, Coverage, Priority, Wanted};
use super::doctrine::{Category, Doctrine};
use super::model::Composition;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Fine,
    Warning,
    Danger,
    Critical,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Fine => "fine",
            Level::Warning => "warning",
            Level::Danger => "danger",
            Level::Critical => "critical",
        }
    }
}

/// One thing worth saying about the fleet as it stands.
#[derive(Clone, PartialEq, Debug)]
pub struct Check {
    pub level: Level,
    /// What was measured, e.g. "Logi".
    pub what: String,
    /// What it found, in the FC's words.
    pub detail: String,
}

/// Logi as a share of the fleet. Below these it is worth knowing before undocking.
pub const LOGI_CRITICAL: f32 = 0.05;
pub const LOGI_DANGER: f32 = 0.10;
pub const LOGI_WARNING: f32 = 0.15;

fn count(comp: &Composition, want: Category) -> usize {
    comp.members().filter(|m| Category::of(&m.ship_group) == want).count()
}

fn band(share: f32) -> Level {
    if share < LOGI_CRITICAL {
        Level::Critical
    } else if share < LOGI_DANGER {
        Level::Danger
    } else if share < LOGI_WARNING {
        Level::Warning
    } else {
        Level::Fine
    }
}

/// How the fleet is for logi, counting only the hulls that can do the job.
///
/// Takes the logi report rather than the composition, so logi that cannot keep up, reps the wrong
/// way or was never in the doctrine does not read as cover.
pub fn logi_from(report: &super::logi::Report) -> Check {
    let (total, n) = (report.fleet, report.counted);
    if total == 0 {
        return Check { level: Level::Fine, what: "Logi".into(), detail: "Nobody in fleet.".into() };
    }
    let share = report.share();
    let level = band(share);
    let want = (LOGI_WARNING * total as f32).ceil() as usize;
    let mut detail = format!("{n} of {total}, {:.0}%.", share * 100.0);
    if !report.rejected.is_empty() {
        detail.push_str(&format!(" {} not counted.", report.rejected.len()));
    }
    if level != Level::Fine {
        detail.push_str(&format!(" {want} would be 15%."));
    }
    Check { level, what: "Logi".into(), detail }
}

/// How the fleet is for logi, as a share of everyone in it.
pub fn logi(comp: &Composition) -> Check {
    let total = comp.total();
    let n = count(comp, Category::Logistics);
    if total == 0 {
        return Check { level: Level::Fine, what: "Logi".into(), detail: "Nobody in fleet.".into() };
    }
    let share = n as f32 / total as f32;
    let level = band(share);
    let want = (LOGI_WARNING * total as f32).ceil() as usize;
    let detail = match level {
        Level::Fine => format!("{n} of {total}, {:.0}%.", share * 100.0),
        _ => format!("{n} of {total}, {:.0}%. {want} would be 15%.", share * 100.0),
    };
    Check { level, what: "Logi".into(), detail }
}

fn count_group(comp: &Composition, group: &str) -> usize {
    comp.members().filter(|m| m.ship_group.trim().eq_ignore_ascii_case(group)).count()
}

/// Whether anything can hold a target down, split into the two hulls that do it: a bubble and a
/// point are different tools and an FC counts them separately.
pub fn interdiction(comp: &Composition) -> Check {
    let dictors = count_group(comp, "Interdictor");
    let hictors = count_group(comp, "Heavy Interdiction Cruiser");
    let level =
        if dictors + hictors == 0 && comp.total() > 0 { Level::Warning } else { Level::Fine };
    let detail = match (dictors, hictors) {
        (0, 0) => "No dictors or hictors in fleet.".to_owned(),
        (d, h) => format!("{d} dictors, {h} hictors."),
    };
    Check { level, what: "Interdiction".into(), detail }
}

/// The hulls that can put a boost up, which is what a missing boost is asked of.
///
/// Command destroyers only count when the doctrine has no command ships in it. A fleet flying
/// command ships is boosting off those, and its destroyers are there to boosh.
pub fn boosters(comp: &Composition, doctrine: Option<&Doctrine>) -> (usize, usize) {
    let ships = count_group(comp, "Command Ship");
    let doctrine_boosts = doctrine.is_some_and(|d| {
        d.ships.iter().any(|s| s.name.trim().eq_ignore_ascii_case("command ship"))
    }) || ships > 0;
    let destroyers = if doctrine_boosts { 0 } else { count_group(comp, "Command Destroyer") };
    (ships, destroyers)
}

/// Whether anything can catch a target.
pub fn tackle(comp: &Composition) -> Check {
    let n = count(comp, Category::Tackle);
    let level = if n == 0 && comp.total() > 0 { Level::Warning } else { Level::Fine };
    let detail = match n {
        0 => "Nothing fast enough to tackle.".to_owned(),
        n => format!("{n} in fleet."),
    };
    Check { level, what: "Tackle".into(), detail }
}

/// Which of the doctrine's boosts nobody is running.
///
/// A missing high-priority boost is worth a colour; a low one is a note. Nothing is set for most
/// doctrines, and a doctrine with no requirements raises nothing rather than a warning about
/// having no requirements.
pub fn boosts(wanted: &[Wanted], have: &[Coverage]) -> Check {
    if wanted.is_empty() {
        return Check {
            level: Level::Fine,
            what: "Boosts".into(),
            detail: "No boosts set for this doctrine.".into(),
        };
    }
    let gaps = boosts::gaps(wanted, have);
    let Some(worst) = gaps.first() else {
        return Check {
            level: Level::Fine,
            what: "Boosts".into(),
            detail: format!("All {} covered.", wanted.len()),
        };
    };
    let level = match worst.priority {
        Priority::High => Level::Danger,
        Priority::Medium => Level::Warning,
        Priority::Low => Level::Fine,
    };
    let names: Vec<String> =
        gaps.iter().map(|g| format!("{} ({})", g.what, g.priority.label().to_lowercase())).collect();
    Check {
        level,
        what: "Boosts".into(),
        detail: format!("Nobody on {}. Run {} next.", names.join(", "), worst.what),
    }
}

/// How many hulls are in fleet that could put a boost up, in the FC's words.
pub fn booster_line(comp: &Composition, doctrine: Option<&Doctrine>) -> String {
    let (ships, destroyers) = boosters(comp, doctrine);
    match (ships, destroyers) {
        (0, 0) => "No command ships in fleet.".to_owned(),
        (s, 0) => format!("{s} command ships."),
        (0, d) => format!("{d} command destroyers."),
        (s, d) => format!("{s} command ships, {d} command destroyers."),
    }
}

/// What the fleet itself is short of, worst first. A fleet nobody has joined is not worth nagging
/// about.
pub fn hulls(comp: &Composition) -> Vec<Check> {
    if comp.total() == 0 {
        return Vec::new();
    }
    let mut out = vec![logi(comp), interdiction(comp), tackle(comp)];
    out.sort_by(|a, b| b.level.cmp(&a.level));
    out
}

/// Everything worth saying, worst first.
pub fn all(
    comp: &Composition,
    _doctrine: Option<&Doctrine>,
    wanted: &[Wanted],
    have: &[Coverage],
) -> Vec<Check> {
    let mut out = hulls(comp);
    if out.is_empty() {
        return out;
    }
    out.push(boosts(wanted, have));
    out.sort_by(|a, b| b.level.cmp(&a.level));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleets::model::{Member, Squad, SquadId, Wing, WingId};

    fn comp(groups: &[(&str, usize)]) -> Composition {
        let mut members = Vec::new();
        let mut id = 0;
        for (group, n) in groups {
            for _ in 0..*n {
                id += 1;
                members.push(Member {
                    character_id: id,
                    name: format!("Pilot {id}"),
                    ship_type_id: id,
                    ship_type_name: "Hull".into(),
                    ship_group: (*group).to_owned(),
                    role: String::new(),
                });
            }
        }
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
        }
    }

    /// The bands the FC asked for: under 5% critical, under 10% danger, under 15% a warning, and
    /// 15% or better is nothing to say.
    #[test]
    fn logi_lands_in_the_right_band() {
        // 3 of 100 is 3%.
        assert_eq!(logi(&comp(&[("Logistics", 3), ("Frigate", 97)])).level, Level::Critical);
        // 7 of 100.
        assert_eq!(logi(&comp(&[("Logistics", 7), ("Frigate", 93)])).level, Level::Danger);
        // 12 of 100.
        assert_eq!(logi(&comp(&[("Logistics", 12), ("Frigate", 88)])).level, Level::Warning);
        // 15 of 100 is the first share that is fine.
        assert_eq!(logi(&comp(&[("Logistics", 15), ("Frigate", 85)])).level, Level::Fine);
        assert_eq!(logi(&comp(&[("Logistics", 40), ("Frigate", 60)])).level, Level::Fine);
    }

    /// Frigate logi counts, and so do force auxiliaries: it is about repairs, not hull size.
    #[test]
    fn every_kind_of_logi_counts() {
        let c = comp(&[("Logistics Frigate", 10), ("Force Auxiliary", 5), ("Frigate", 85)]);
        assert_eq!(logi(&c).level, Level::Fine);
    }

    /// The report's count is what the band is read off, not the hull count.
    #[test]
    fn unusable_logi_does_not_count_as_cover() {
        use crate::fleets::logi::{Reason, Rejected, Report, Size};
        let report = Report {
            counted: 1,
            fleet: 20,
            rejected: vec![
                Rejected { pilot: "A".into(), ship: "Guardian".into(), why: Reason::WrongTank },
                Rejected { pilot: "B".into(), ship: "Kirin".into(), why: Reason::TooSmall },
            ],
            tank: None,
            size: Size::Line,
        };
        let check = logi_from(&report);
        assert_eq!(check.level, Level::Danger, "three hulls but only one of them reps");
        assert!(check.detail.contains("1 of 20"), "{}", check.detail);
        assert!(check.detail.contains("2 not counted"), "{}", check.detail);
        assert!(check.detail.contains("3 would be 15%"), "{}", check.detail);

        // All of them usable is the same fleet reading Fine.
        let good = Report { counted: 3, fleet: 20, rejected: Vec::new(), ..report };
        assert_eq!(logi_from(&good).level, Level::Fine);
        assert!(!logi_from(&good).detail.contains("not counted"));
    }

    /// A thin fleet says what would not be thin.
    #[test]
    fn a_thin_fleet_says_what_it_would_take() {
        let c = comp(&[("Logistics", 1), ("Frigate", 19)]);
        let check = logi(&c);
        assert_eq!(check.level, Level::Danger);
        assert!(check.detail.contains("1 of 20"), "{}", check.detail);
        assert!(check.detail.contains("3 would be 15%"), "{}", check.detail);
    }

    /// Bubbles and points are counted apart, because they are different tools.
    #[test]
    fn interdiction_counts_dictors_and_hictors_apart() {
        let c = comp(&[
            ("Battleship", 10),
            ("Interdictor", 2),
            ("Heavy Interdiction Cruiser", 1),
        ]);
        let check = interdiction(&c);
        assert_eq!(check.level, Level::Fine);
        assert!(check.detail.contains("2 dictors"), "{}", check.detail);
        assert!(check.detail.contains("1 hictors"), "{}", check.detail);
        assert_eq!(
            interdiction(&comp(&[("Battleship", 10)])).detail,
            "No dictors or hictors in fleet."
        );
    }

    /// A fleet flying command ships boosts off those, so its destroyers are booshers.
    #[test]
    fn command_destroyers_only_count_where_nothing_else_boosts() {
        let both = comp(&[("Command Ship", 2), ("Command Destroyer", 3), ("Battleship", 10)]);
        assert_eq!(boosters(&both, None), (2, 0));
        assert_eq!(booster_line(&both, None), "2 command ships.");

        let destroyers_only = comp(&[("Command Destroyer", 3), ("Battleship", 10)]);
        assert_eq!(boosters(&destroyers_only, None), (0, 3));
        assert_eq!(booster_line(&destroyers_only, None), "3 command destroyers.");

        assert_eq!(booster_line(&comp(&[("Battleship", 10)]), None), "No command ships in fleet.");

        // A doctrine that flies command ships discounts the destroyers even before one undocks.
        let d = crate::fleets::doctrine::Doctrine {
            ships: vec![crate::fleets::doctrine::DoctrineShip {
                type_id: 0,
                name: "Command Ship".into(),
                tank: None,
            }],
            ..Default::default()
        };
        assert_eq!(boosters(&destroyers_only, Some(&d)), (0, 0));
    }

    /// Nobody to hold a target, and nobody to catch one.
    #[test]
    fn interdiction_and_tackle_are_noticed_when_missing() {
        let bare = comp(&[("Battleship", 20)]);
        assert_eq!(interdiction(&bare).level, Level::Warning);
        assert_eq!(tackle(&bare).level, Level::Warning);

        let equipped = comp(&[("Battleship", 18), ("Interdictor", 1), ("Interceptor", 1)]);
        assert_eq!(interdiction(&equipped).level, Level::Fine);
        assert_eq!(tackle(&equipped).level, Level::Fine);

        // A hictor holds things down too.
        let hic = comp(&[("Battleship", 19), ("Heavy Interdiction Cruiser", 1)]);
        assert_eq!(interdiction(&hic).level, Level::Fine);
    }

    /// An empty fleet is not worth nagging about.
    #[test]
    fn an_empty_fleet_raises_nothing() {
        assert!(all(&comp(&[]), None, &[], &[]).is_empty());
    }

    /// A missing boost is as loud as the doctrine says it is, and names what to put on.
    #[test]
    fn a_missing_boost_is_as_loud_as_its_priority() {
        let wanted = |p: Priority| vec![Wanted { what: "Shield Extension".into(), priority: p }];
        assert_eq!(boosts(&wanted(Priority::High), &[]).level, Level::Danger);
        assert_eq!(boosts(&wanted(Priority::Medium), &[]).level, Level::Warning);
        assert_eq!(boosts(&wanted(Priority::Low), &[]).level, Level::Fine);

        let check = boosts(&wanted(Priority::High), &[]);
        assert!(check.detail.contains("Run Shield Extension next"), "{}", check.detail);

        // A doctrine nobody has set requirements for says nothing.
        assert_eq!(boosts(&[], &[]).level, Level::Fine);

        let covered = crate::fleets::boosts::coverage(
            &[crate::fleets::boosts::parse("P", "Shield Extension Charge", 1).expect("declared")],
            0,
        );
        let check = boosts(&wanted(Priority::High), &covered);
        assert_eq!(check.level, Level::Fine);
        assert!(check.detail.contains("All 1 covered"), "{}", check.detail);
    }

    /// Worst first, so the one that matters is the one that is read.
    #[test]
    fn the_worst_check_comes_first() {
        let c = comp(&[("Logistics", 1), ("Battleship", 99)]);
        let checks = all(&c, None, &[], &[]);
        assert_eq!(checks[0].what, "Logi");
        assert_eq!(checks[0].level, Level::Critical);
        assert!(checks.iter().any(|c| c.what == "Interdiction"));
    }
}
