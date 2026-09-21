//! The reference data the dry run answers from.
//!
//! The real ids and names are alliance-internal, so they are not in this repo. They load from a
//! JSON file in the profile directory (override with `EVE_SPAI_FLEET_SEED`); without it the tab
//! runs on placeholders that have the right shape and the right id numbers but invented names.

use serde::{Deserialize, Serialize};

use super::model::*;

/// Everything the tab needs before it can render a form.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Seed {
    pub identity: Identity,
    pub characters: Vec<AccountCharacter>,
    pub sigs: Vec<Labelled>,
    pub setups: Vec<SetupItem>,
    pub mumble_channels: Vec<ChannelItem>,
    pub logi_channels: Vec<ChannelItem>,
    pub boost_channels: Vec<ChannelItem>,
    pub tags: Vec<TagItem>,
    /// Systems offered by the formup type-ahead.
    pub systems: Vec<Labelled>,
    /// The hulls each setup flies, keyed by setup id.
    pub doctrines: Vec<SeedDoctrine>,
    /// True when these are placeholders rather than the real tables. Only the dry-run spoof sets
    /// it: the app itself never carries invented data.
    #[serde(skip)]
    pub placeholder: bool,
}

impl Seed {
    /// Whether the reference tables have been filled at all.
    pub fn empty(&self) -> bool {
        self.setups.is_empty() && self.mumble_channels.is_empty() && self.tags.is_empty()
    }
}

/// One setup's hulls, as the seed file spells them.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SeedDoctrine {
    pub setup_id: i32,
    /// (type id, name) per hull the doctrine asks for.
    pub ships: Vec<(i64, String)>,
}

fn setup(id: i32, name: &str, opsec: Option<&str>) -> SetupItem {
    SetupItem {
        id: SetupId(id),
        name: name.to_owned(),
        minimal_opsec_level_description: opsec.map(str::to_owned),
        priority: 0,
        is_default: false,
    }
}

fn channel(id: i32, name: &str, in_use: bool) -> ChannelItem {
    ChannelItem { id: ChannelId(id), name: name.to_owned(), is_in_use: in_use }
}

fn tag(id: i32, name: &str, colour: &str, primary: bool, strategic: bool) -> TagItem {
    TagItem {
        id: TagId(id),
        name: name.to_owned(),
        colour_class: colour.to_owned(),
        is_primary: primary,
        is_strategic: strategic,
    }
}

/// Placeholders. The ids are the real ones, because the payload tests and a later switch to the
/// real backend both care about them; every name here is invented.
///
/// Test and screenshot scaffolding only. Nothing the app runs reaches this: `Seed::default()` is
/// empty and `load()` returns empty, so an unfilled table reads as unfilled.
pub fn invented() -> Seed {
    Seed {
        identity: Identity {
            name: "Placeholder FC".to_owned(),
            command_group: "FC".to_owned(),
            sigs: vec![],
            // Everything, so the UI can be driven. A narrower identity is what the tests use.
            permissions: [
                Perm::AccessFleet,
                Perm::AccessFleetModule,
                Perm::StartFleet,
                Perm::InviteMember,
                Perm::KickMember,
                Perm::MoveMember,
                Perm::ManageFleetSnowflakes,
                Perm::FlagFleet,
                Perm::AccessPayouts,
                Perm::AccessCommanderStats,
                Perm::AccessLogiAnchorStats,
                Perm::AccessStatisticsModule,
            ]
            .iter()
            .map(|p| p.as_str().to_owned())
            .collect(),
        },
        characters: vec![
            AccountCharacter {
                id: 90_000_001,
                name: "Placeholder Main".to_owned(),
                corporation_id: 98_000_001,
                is_hidden: false,
            },
            AccountCharacter {
                id: 90_000_002,
                name: "Placeholder Alt".to_owned(),
                corporation_id: 98_000_001,
                is_hidden: false,
            },
        ],
        sigs: vec![Labelled { id: 1871, label: "[S] Placeholder SIG".to_owned() }],
        setups: vec![
            setup(61, "FC Choice", None),
            setup(19, "Entosis", Some("FC")),
            setup(30, "Destroyers", Some("FC")),
            setup(46, "Fast Tackle", Some("FC")),
            setup(51, "Frigates", Some("FC")),
            setup(60, "Bombers", Some("FC")),
            setup(65, "Assault Frigates", Some("FC")),
            setup(84, "Battlecruisers", Some("FC")),
            setup(100, "Cruisers", Some("FC")),
            setup(116, "Interceptors", Some("FC")),
            setup(125, "Battleships", Some("FC")),
            setup(128, "Tactical Destroyers", Some("FC")),
            setup(140, "Heavy Battleships", Some("FC")),
            setup(149, "Incursions", None),
        ],
        mumble_channels: (1..=6)
            .map(|i| channel(i, &format!("Comms {i}"), i <= 2))
            .chain([
                channel(7, "Lobby", false),
                channel(10, "Home Defence", false),
                channel(11, "Capitals", true),
                channel(12, "Comms 11", false),
                channel(15, "Camp", false),
                channel(16, "Standing", false),
            ])
            .collect(),
        logi_channels: (1..=8)
            .map(|i| channel(i, &format!("Logi {i}"), i <= 2))
            .collect(),
        boost_channels: (1..=6)
            .map(|i| channel(i, &format!("Boosts {i}"), i == 1))
            .collect(),
        tags: vec![
            tag(1, "STRATEGIC", "red", true, true),
            tag(2, "PEACETIME", "green", true, false),
            tag(3, "SIG/SQUAD", "yellow", true, false),
            tag(20, "Squad A", "yellow", true, true),
            tag(21, "Squad B", "yellow", true, true),
            tag(22, "Squad C", "yellow", true, false),
            tag(27, "Corp", "yellow", true, false),
            tag(37, "Group A", "dark", true, false),
            tag(41, "Group B", "dark", true, false),
            tag(49, "Incursions", "yellow", true, false),
            tag(4, "Gatecamp", "dark", false, false),
            tag(12, "Home Defence", "blue", false, false),
            tag(18, "Move op", "dark", false, false),
            tag(19, "Training", "dark", false, false),
            tag(28, "Capital Save", "blue", false, false),
            tag(29, "Structure Defence", "blue", false, false),
            tag(30, "ESS Defence", "blue", false, false),
            tag(31, "Structure Bash", "dark", false, false),
            tag(32, "PVE", "light", false, false),
            tag(33, "Roam", "dark", false, false),
            tag(34, "Whaling", "dark", false, false),
            tag(35, "Other", "light", false, false),
            tag(36, "Standing Fleet", "light", false, false),
            tag(60, "Entosis", "blue", false, false),
            tag(63, "ADM", "blue", false, false),
        ],
        systems: vec![
            Labelled { id: 30_000_772, label: "Placeholder Staging".to_owned() },
            Labelled { id: 30_000_142, label: "Jita".to_owned() },
        ],
        // Real hull ids, so a fleet read through ESI lines up with them.
        doctrines: vec![
            SeedDoctrine {
                setup_id: 46,
                ships: vec![
                    (22_464, "Flycatcher".to_owned()),
                    (37_458, "Kirin".to_owned()),
                    (11_381, "Harpy".to_owned()),
                ],
            },
            SeedDoctrine {
                setup_id: 116,
                ships: vec![(11_176, "Crow".to_owned()), (11_184, "Crusader".to_owned())],
            },
            SeedDoctrine {
                setup_id: 125,
                ships: vec![(644, "Typhoon".to_owned()), (11_987, "Guardian".to_owned())],
            },
        ],
        placeholder: true,
    }
}

/// Where the real table is read from.
pub fn path() -> Option<std::path::PathBuf> {
    match std::env::var("EVE_SPAI_FLEET_SEED") {
        Ok(p) if !p.trim().is_empty() => Some(std::path::PathBuf::from(p)),
        _ => crate::store::data_dir().ok().map(|d| d.join("fleet-seed.json")),
    }
}

/// A seed file's contents. Anything it leaves out stays empty, so a hand-written file can carry
/// only the parts someone cared about and the rest is plainly missing rather than plausibly wrong.
pub fn parse(text: &str) -> std::result::Result<Seed, String> {
    let mut seed: Seed = serde_json::from_str(text).map_err(|e| e.to_string())?;
    seed.placeholder = false;
    Ok(seed)
}

/// The real table if there is one, nothing if not.
///
/// Never the invented table. A made-up channel name is indistinguishable from a real one on
/// screen, and an FC who pings the wrong comms because the app filled a gap with a plausible
/// guess is worse off than one who sees the gap.
pub fn load() -> Seed {
    let Some(p) = path() else { return Seed::default() };
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        // Absent is the normal case before a sign-in fills the tables from the API.
        Err(_) => return Seed::default(),
    };
    match parse(&text) {
        Ok(seed) => seed,
        Err(e) => {
            crate::esilog::record("fleet seed unreadable", &format!("{}: {e}", p.display()));
            Seed::default()
        }
    }
}

impl Seed {
    pub fn setup_name(&self, id: SetupId) -> Option<&str> {
        self.setups.iter().find(|s| s.id == id).map(|s| s.name.trim())
    }

    pub fn channel_name<'a>(&self, list: &'a [ChannelItem], id: Option<ChannelId>) -> Option<&'a str> {
        let id = id?;
        list.iter().find(|c| c.id == id).map(|c| c.name.trim())
    }

    /// The comms channel called "Op N", for an op number the FC typed.
    ///
    /// The id is NOT the op number. They agree up to Op 6 and then diverge: id 7 is "o7", id 8 is
    /// "Op 9", id 11 is "Capital Comms" and "Op 11" is id 12. Sending the number as an id put a
    /// rescue ping on Capital Comms while every screen still said Op 11, so this matches on the
    /// name and answers None rather than guessing.
    pub fn mumble_channel_for_op(&self, op: u8) -> Option<ChannelId> {
        let want = format!("op {op}");
        self.mumble_channels
            .iter()
            .find(|c| c.name.trim().eq_ignore_ascii_case(&want))
            .map(|c| c.id)
    }

    pub fn tag(&self, id: TagId) -> Option<&TagItem> {
        self.tags.iter().find(|t| t.id == id)
    }

    /// What a setup flies, if the seed says.
    pub fn doctrine(&self, id: SetupId) -> Option<super::doctrine::Doctrine> {
        let d = self.doctrines.iter().find(|d| d.setup_id == id.0)?;
        Some(super::doctrine::Doctrine {
            setup_id: id,
            setup_name: self.setup_name(id).unwrap_or_default().to_owned(),
            ships: d
                .ships
                .iter()
                .map(|(tid, name)| super::doctrine::DoctrineShip {
                    type_id: *tid,
                    name: name.clone(),
                    main: false,
                })
                .collect(),
            support: Vec::new(),
            tank: None,
            strict: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The placeholder table has to be complete enough to drive every control, or the dry run
    /// cannot exercise the code paths it exists to exercise.
    #[test]
    fn the_placeholder_seed_fills_every_control() {
        let s = invented();
        assert!(s.placeholder);
        assert!(s.setups.len() >= 10);
        assert_eq!(s.logi_channels.len(), 8);
        assert_eq!(s.boost_channels.len(), 6);
        assert!(s.tags.iter().any(|t| t.is_primary) && s.tags.iter().any(|t| !t.is_primary));
        assert!(s.tags.iter().any(|t| t.is_strategic));
        assert!(!s.characters.is_empty() && !s.sigs.is_empty() && !s.systems.is_empty());
        assert!(s.identity.can(Perm::StartFleet));
    }

    /// Some channels are busy, or the free-channel rule has nothing to do.
    #[test]
    fn the_placeholder_seed_has_busy_and_free_channels() {
        let s = invented();
        for list in [&s.mumble_channels, &s.logi_channels, &s.boost_channels] {
            assert!(list.iter().any(|c| c.is_in_use), "nothing is busy");
            assert!(list.iter().any(|c| !c.is_in_use), "nothing is free");
        }
    }

    /// The ids are the real ones even though the names are not, because a later switch to the real
    /// backend has to line up.
    #[test]
    fn the_placeholder_ids_are_the_real_ones() {
        let s = invented();
        assert!(s.setups.iter().any(|x| x.id == SetupId(61)));
        assert_eq!(s.tag(TagId(1)).map(|t| t.name.as_str()), Some("STRATEGIC"));
        assert!(s.tag(TagId(60)).is_some_and(|t| !t.is_primary), "60 is a tag, not the setup");
        assert!(s.setups.iter().any(|x| x.id == SetupId(60)), "60 is also a setup");
    }

    /// A seed file round-trips, so the documented shape is the one `load` reads. Anything that
    /// parsed from a file is by definition not a placeholder, even though the struct's Default is.
    #[test]
    fn a_seed_file_round_trips_and_is_not_a_placeholder() {
        let s = invented();
        let text = serde_json::to_string(&s).expect("serialise");
        let back = parse(&text).expect("parse");
        assert_eq!(back.setups, s.setups);
        assert_eq!(back.tags, s.tags);
        assert!(!back.placeholder);
    }

    /// A partial file is filled in rather than rejected, so a hand-written seed can carry only the
    /// tables someone cared about.
    #[test]
    fn a_partial_seed_file_leaves_the_rest_empty() {
        let back = parse(r#"{"sigs":[{"id":5,"label":"Mine"}]}"#).expect("parse");
        assert_eq!(back.sigs.len(), 1);
        // Not the invented table. A channel name the app made up is indistinguishable from a real
        // one on screen, and pinging the wrong comms is worse than seeing an empty dropdown.
        assert!(back.setups.is_empty(), "the rest stays empty");
        assert!(back.mumble_channels.is_empty());
        assert!(!back.placeholder);
        // Sigs alone are not reference data: nothing the form needs came with it.
        assert!(back.empty(), "no setups, channels or tags is unusable");
        assert!(Seed::default().empty());
        assert!(!invented().empty(), "the test table is usable, that is its job");
    }

    /// A broken file must not be mistaken for real data.
    #[test]
    fn a_broken_seed_file_is_refused() {
        assert!(parse("{not json").is_err());
    }
}

#[cfg(test)]
mod op_channel_tests {
    use super::*;

    fn seed() -> Seed {
        let names = [
            (1, "Op 1"), (2, "Op 2"), (3, "Op 3"), (4, "Op 4"), (5, "Op 5"), (6, "Op 6"),
            (7, "o7"), (8, "Op 9"), (9, "Op 10"), (10, "HD"), (11, "Capital Comms"),
            (12, "Op 11"), (13, "Op 12"),
        ];
        Seed {
            mumble_channels: names
                .into_iter()
                .map(|(id, name)| ChannelItem {
                    id: ChannelId(id),
                    name: name.to_owned(),
                    is_in_use: false,
                })
                .collect(),
            ..Seed::default()
        }
    }

    /// The op number is not the channel id. They agree up to Op 6 and diverge after, and a rescue
    /// that sent the number as an id put its ping on Capital Comms while every screen said Op 11.
    #[test]
    fn an_op_number_resolves_by_name_not_by_id() {
        let s = seed();
        assert_eq!(s.mumble_channel_for_op(1), Some(ChannelId(1)));
        assert_eq!(s.mumble_channel_for_op(6), Some(ChannelId(6)));
        // The ones that used to be silently wrong.
        assert_eq!(s.mumble_channel_for_op(9), Some(ChannelId(8)));
        assert_eq!(s.mumble_channel_for_op(10), Some(ChannelId(9)));
        assert_eq!(s.mumble_channel_for_op(11), Some(ChannelId(12)));
        assert_eq!(s.mumble_channel_for_op(12), Some(ChannelId(13)));
        // Nothing resolves to the channel the old arithmetic picked.
        assert!((1..=12).all(|n| s.mumble_channel_for_op(n) != Some(ChannelId(11))));
    }

    /// An op with no channel answers None. Sending nothing is recoverable; naming the wrong
    /// channel sends a fleet to the wrong comms and nobody finds out until it matters.
    #[test]
    fn an_op_with_no_channel_is_not_guessed() {
        let s = seed();
        assert_eq!(s.mumble_channel_for_op(7), None);
        assert_eq!(s.mumble_channel_for_op(8), None);
        assert_eq!(s.mumble_channel_for_op(99), None);
    }
}
