//! Doctrine names in fleet pings, and the forum topic that holds the fits.
//!
//! The names in a ping are written by the FC, so they are matched loosely: "Hammer Fleet (FNI)
//! (Boosters > Ferox Navy Issue > ...)" and "FNI" are the same doctrine.

/// One forum topic and the names that mean it.
struct Doctrine {
    url: &'static str,
    /// Matched anywhere in the name.
    any: &'static [&'static str],
    /// Matched as whole words. Short names like ENI, SIR or tFI sit inside ordinary words, so they
    /// cannot be substrings.
    words: &'static [&'static str],
}

const fn d(
    url: &'static str,
    any: &'static [&'static str],
    words: &'static [&'static str],
) -> Doctrine {
    Doctrine { url, any, words }
}

/// Strategic first, then peacetime, then the doctrine index. Order only decides between two entries
/// that both match, which the keys are chosen to avoid: they name the doctrine, not the ships in its
/// fleet, since half these fleets fly the same logistics.
const DOCTRINES: &[Doctrine] = &[
    // Strategic
    d("https://goonfleet.com/index.php/topic/380930-active-strat-typhoons/", &["typhoon"], &[]),
    d("https://goonfleet.com/index.php/topic/369029-active-strat-vultures/", &["vulture"], &[]),
    d(
        "https://goonfleet.com/index.php/topic/355156-active-strat-tomahawks-ravens/",
        &["tomahawk", "raven"],
        &[],
    ),
    d("https://goonfleet.com/index.php/topic/326958-active-strat-flycatchers/", &["flycatcher"], &[]),
    d("https://goonfleet.com/index.php/topic/366187-active-strat-snail-fleet/", &["snail"], &["moa", "moas"]),
    d(
        "https://goonfleet.com/index.php/topic/376533-active-strat-tigers-claw-carriers/",
        &["tiger"],
        &["carrier", "carriers"],
    ),
    d(
        "https://goonfleet.com/index.php/topic/379775-active-beehive-stupid-idiot-rorquals-sir/",
        // Not "rorqual" on its own: a mining ping lists one at the end of its ship priorities, and
        // this doctrine is a combat fleet.
        &["stupid idiot", "beehive", "combat rorqual"],
        &["sir"],
    ),
    d(
        "https://goonfleet.com/index.php/topic/374474-active-strat-alpha-fleet-maelstroms/",
        &["maelstrom", "alpha fleet"],
        &[],
    ),
    d(
        "https://goonfleet.com/index.php/topic/372184-active-strat-contraceptors/",
        &["contraceptor", "crusader", "malediction"],
        &[],
    ),
    d("https://goonfleet.com/index.php/topic/349390-active-strat-eni-fleet/", &["exequror"], &["eni", "enis"]),
    d("https://goonfleet.com/index.php/topic/351207-active-strat-entosis-ships-2024/", &["entosis"], &[]),
    d(
        "https://goonfleet.com/index.php/topic/357801-active-strat-hammerfleet-fni/",
        &["hammer", "ferox navy"],
        &["fni", "fnis"],
    ),
    d("https://goonfleet.com/index.php/topic/346057-active-strat-harpyfleet/", &["harpy"], &[]),
    d(
        "https://goonfleet.com/index.php/topic/381392-active-strat-sacrileges/",
        // Both spellings: the pings are written by hand.
        &["sacrileg", "sacreleg"],
        &[],
    ),
    d(
        "https://goonfleet.com/index.php/topic/344458-active-strat-siegefleet-40-torp-bombers/",
        &["siegefleet", "siege fleet", "torp bomber", "torpedo bomber"],
        &[],
    ),
    d("https://goonfleet.com/index.php/topic/374663-active-strat-svipuls/", &["svipul"], &[]),
    d(
        "https://goonfleet.com/index.php/topic/345055-active-strat-void-rays-mwd-kikis/",
        &["void ray", "kiki"],
        &[],
    ),
    d(
        "https://goonfleet.com/index.php/topic/293352-active-cyno-recon-and-black-ops-fits/",
        &["black ops", "blops"],
        &["recon", "recons"],
    ),
    d(
        "https://goonfleet.com/index.php/topic/353977-active-suggested-utility-dictorboosher-fits/",
        &["boosher"],
        &[],
    ),
    // Peacetime
    d("https://goonfleet.com/index.php/topic/371893-active-peacetime-ruptures/", &["rupture"], &[]),
    d(
        "https://goonfleet.com/index.php/topic/354268-active-peacetime-retributions/",
        &["retribution", "retri"],
        &[],
    ),
    d(
        "https://goonfleet.com/index.php/topic/295648-active-peacetime-mosh-masters-hurricanes/",
        &["mosh", "hurricane"],
        &[],
    ),
    d(
        "https://goonfleet.com/index.php/topic/341744-active-peacetime-osprey-navy-issues/",
        // Not "osprey": half the fleets in here bring one as logistics.
        &["osprey navy"],
        &["oni", "onis"],
    ),
    d(
        "https://goonfleet.com/index.php/topic/349435-active-peacetime-tfis/",
        &["thrasher"],
        &["tfi", "tfis"],
    ),
    d("https://goonfleet.com/index.php/topic/314644-active-peacetime-kestrels/", &["kestrel"], &[]),
    d("https://goonfleet.com/index.php/topic/299033-active-peacetime-cormorants/", &["cormorant"], &[]),
    // The index the rest of them live in.
    d(
        "https://goonfleet.com/index.php/topic/349468-goonswarm-federation-unified-strategic-doctrine-mk-xiii/",
        &["unified", "mk xiii", "strategic doctrine"],
        &[],
    ),
];

/// The name a ping's doctrine field is matched on: the first line, up to the ship priorities.
///
/// A doctrine field reads "Harpy Fleet (Boosters > Kirin/Scalpel > Harpy > Else)" and often carries
/// the FC's own remark on a second line. Everything after the first bracket names ships other fleets
/// fly too, and the remark names anything at all, so neither gets to decide which topic this is.
fn name_of(doctrine: &str) -> String {
    let line = doctrine.lines().next().unwrap_or_default();
    let head = line.split('(').next().unwrap_or_default().trim();
    let pick = if head.len() < 3 { line } else { head };
    pick.to_lowercase()
}

/// Whether `name` carries `word` as a word rather than inside one.
fn has_word(name: &str, word: &str) -> bool {
    name.split(|c: char| !c.is_alphanumeric()).any(|w| w == word)
}

pub fn link_for(doctrine: &str) -> Option<&'static str> {
    let name = name_of(doctrine);
    DOCTRINES
        .iter()
        .find(|d| {
            d.any.iter().any(|k| name.contains(k)) || d.words.iter().any(|w| has_word(&name, w))
        })
        .map(|d| d.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The doctrine fields as they are actually written, one per active topic.
    #[test]
    fn every_doctrine_in_a_ping_finds_its_topic() {
        let cases = [
            ("Typhoon (Boosters > Guardian > Typhoon > HIC > Confessor > Else)", "typhoons"),
            ("Vultures (Booster > Basi > Vulture > Onyx > Lachesis/Huginn > Svipul > Else)", "vultures"),
            ("Tomahawks (Booster > Basilisk > RAVEN > Support > Else)", "tomahawks-ravens"),
            ("Ravens", "tomahawks-ravens"),
            ("Flycatchers (Boosters > Kirin/Scalpel > Flycatcher > Else)", "flycatchers"),
            ("Snail Fleet (Moas) (Moa > Moa > Osprey > Booster Drake)", "snail-fleet"),
            ("Tiger's Claw (Carriers)", "tigers-claw"),
            ("SIR Fleet (Combat Rorqual > Faxes > Hictors > Dictors)", "rorquals-sir"),
            ("Stupid Idiot Rorquals (Rorquals > FAX > hics/dics)", "rorquals-sir"),
            ("Maelstrom (Booster > Basilisk > Maelstrom > Support > Else)", "alpha-fleet"),
            ("Crusaders (CHECK YOUR POD >>>>> Crusader)", "contraceptors"),
            ("ENIS (Booster > Deacon > ENI)", "eni-fleet"),
            ("Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)", "hammerfleet"),
            ("FNI", "hammerfleet"),
            ("Harpy Fleet (Boosters > Kirin/Scalpel > Harpy > Else)", "harpyfleet"),
            ("Sacrileges (Guardian > Sacrelige > Legion > Confessor | Devoter)", "sacrileges"),
            ("Torp Bombers (Purifier > Hound > Astero > Else)", "siegefleet"),
            ("Svipul (Boosters > Kirin/Scalpel > Svipul > Else)", "svipuls"),
            ("Void Rays (MWD kikis) (Boosts > Logi > Kikis > Slasher/Hyena/Keres)", "void-rays"),
            ("Rupture (Boosters > Rupture > Osprey)", "ruptures"),
            ("Retri Fleet (Booster > Deacon > Retri > Support)", "retributions"),
            ("Mosh Masters (Boosters > Osprey/Scythe > Hurricane > Hyena > Else)", "mosh-masters"),
            ("Osprey Navy Issue (Boosters > ONI > Scythe > Else)", "osprey-navy-issues"),
            ("thrasher Fleet Issues (tFI > scalpel > sentinel )", "tfis"),
            ("Kestrels (Kestrels>Bifrosts>Vigils)", "kestrels"),
            ("Cormorant (Corm > Burst > Bantam > Vigil)", "cormorants"),
            // Entosis fleets ping the doctrine as the bare word.
            ("Entosis", "entosis-ships"),
        ];
        for (doctrine, want) in cases {
            let got = link_for(doctrine).unwrap_or_else(|| panic!("no link for {doctrine:?}"));
            assert!(got.contains(want), "{doctrine:?} linked to {got}, wanted the {want} topic");
        }
    }

    /// Tiger's Claw has not been pinged since this table was written, so both ways of naming it are
    /// covered: the topic's own name, and the ships it flies.
    #[test]
    fn the_carrier_doctrine_answers_to_either_name() {
        for name in [
            "Tiger's Claw (Carriers)",
            "Tigers Claw",
            "Carriers (Thanatos > Fax > Support)",
            "Carrier Fleet",
        ] {
            let got = link_for(name).unwrap_or_else(|| panic!("no link for {name:?}"));
            assert!(got.contains("tigers-claw"), "{name:?} linked to {got}");
        }
    }

    /// The ships after the bracket are the fleet's, not the doctrine's: half of these fly the same
    /// logistics and the same tackle, and a fleet named after one of them would link to the wrong
    /// topic.
    #[test]
    fn the_ships_in_the_priorities_do_not_decide() {
        let snail = link_for("Snail Fleet (Moas) (Moa > Moa > Osprey > Booster Drake)").unwrap();
        assert!(snail.contains("snail"), "an Osprey in the priorities pulled it to another topic");
        let vultures =
            link_for("Vultures (Booster > Basi > Vulture > Onyx > Lachesis/Huginn > Svipul > Else)")
                .unwrap();
        assert!(vultures.contains("vultures"), "the Svipul in the priorities decided instead");
        let rupture = link_for("Rupture (Boosters > Rupture > Osprey)").unwrap();
        assert!(rupture.contains("ruptures"));
    }

    /// The FC's remark rides on a second line and names whatever it likes.
    #[test]
    fn a_remark_under_the_name_does_not_decide() {
        let got = link_for("Flycatchers (Boosters > Kirin/Scalpel > Flycatcher > Else)\nneed ceptors too!")
            .unwrap();
        assert!(got.contains("flycatchers"), "the remark's ceptors decided instead, got {got}");
    }

    /// A fleet with no doctrine gets no link rather than the nearest ship name.
    #[test]
    fn a_fleet_without_a_doctrine_links_nowhere() {
        assert_eq!(link_for("*FC Choice* (Fits in MOTD)"), None);
        assert_eq!(link_for("None - This is a theory class"), None);
        assert_eq!(link_for("Kitchen Sink BS and below"), None);
        assert_eq!(link_for("T1/T2 EWAR Frigates"), None);
        assert_eq!(
            link_for("Exhumers > Barges > Mining Frigates > Porpoises > Rorqual"),
            None,
            "a mining op is not the combat rorqual doctrine"
        );
        assert_eq!(link_for("Osprey, Guardian, Basilisk"), None, "logistics is not a doctrine");
    }

    /// Short names are words, or they turn up inside longer ones.
    #[test]
    fn short_names_are_matched_as_words() {
        assert!(link_for("ENI").unwrap().contains("eni-fleet"));
        assert!(link_for("ONI").unwrap().contains("osprey-navy"));
        assert_eq!(link_for("Vedmak->Rodiva->Tackle->Else"), None, "no doctrine hides in these names");
    }

    /// A doctrine that points at its own thread still resolves, since the name is read first.
    #[test]
    fn a_name_that_carries_a_link_still_matches() {
        let got = link_for(
            "Logi-Ospreys, see Snail doctrine (https://goonfleet.com/index.php/topic/366187-active-strat-snail-fleet/)",
        )
        .unwrap();
        assert!(got.contains("snail-fleet"));
    }
}
