//! Static NPC "rat" pirate-faction data per region. EVE's rat faction is
//! consistent within a region, but the SDE's region `factionID` is empty for most
//! player-sov nullsec, so this is a curated table of high-confidence regions only
//! (uncertain regions return None rather than guess).

/// A pirate faction and how it fights (for ratting/tanking decisions).
#[derive(Clone, Copy)]
pub struct RatProfile {
    pub faction: &'static str,
    /// Damage types they deal — tank against these (primary, secondary).
    pub deal: [&'static str; 2],
    /// Damage types they're weakest to — deal these (primary, secondary).
    pub weak: [&'static str; 2],
    /// Dominant electronic-warfare they use.
    pub ewar: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
enum Faction {
    Guristas,
    Angel,
    Blood,
    Sansha,
    Serpentis,
    RogueDrones,
    Triglavian,
}

fn profile(f: Faction) -> RatProfile {
    use Faction::*;
    match f {
        Guristas => RatProfile {
            faction: "Guristas",
            deal: ["Kinetic", "Thermal"],
            weak: ["Kinetic", "Thermal"],
            ewar: "ECM (jamming)",
        },
        Angel => RatProfile {
            faction: "Angel Cartel",
            deal: ["Explosive", "Kinetic"],
            weak: ["Explosive", "Kinetic"],
            ewar: "Target painting",
        },
        Blood => RatProfile {
            faction: "Blood Raiders",
            deal: ["EM", "Thermal"],
            weak: ["EM", "Thermal"],
            ewar: "Energy neutralizing / nos",
        },
        Sansha => RatProfile {
            faction: "Sansha's Nation",
            deal: ["EM", "Thermal"],
            weak: ["EM", "Thermal"],
            ewar: "Tracking disruption",
        },
        Serpentis => RatProfile {
            faction: "Serpentis",
            deal: ["Thermal", "Kinetic"],
            weak: ["Thermal", "Kinetic"],
            ewar: "Sensor dampening",
        },
        RogueDrones => RatProfile {
            faction: "Rogue Drones",
            deal: ["Varies", "—"],
            weak: ["Varies", "—"],
            ewar: "None",
        },
        Triglavian => RatProfile {
            faction: "Triglavian Collective",
            deal: ["Thermal", "EM"],
            weak: ["Thermal", "Explosive"],
            ewar: "Warp disruption / neut",
        },
    }
}

fn region_faction(region: &str) -> Option<Faction> {
    use Faction::*;
    let f = match region {
        // Caldari / Guristas
        "The Forge" | "Lonetrek" | "The Citadel" | "Black Rise" | "Venal" | "Tenal" => Guristas,
        // Minmatar / Angel Cartel
        "Heimatar" | "Metropolis" | "Molden Heath" | "Great Wildlands" | "Curse"
        | "Scalding Pass" | "Wicked Creek" | "Insmother" | "Detorid" | "Tenerifis" | "Omist"
        | "Feythabolis" | "Immensea" => Angel,
        // Amarr / Blood Raiders
        "Kor-Azor" | "Kador" | "Aridia" | "Khanid" | "Delve" | "Period Basis" | "Querious"
        | "Paragon Soul" => Blood,
        // Amarr / Sansha's Nation
        "Domain" | "Tash-Murkon" | "Devoid" | "Stain" | "Catch" => Sansha,
        // Gallente / Serpentis
        "Sinq Laison" | "Essence" | "Verge Vendor" | "Placid" | "Everyshore" | "Solitude"
        | "Fountain" | "Cloud Ring" | "Syndicate" | "Outer Ring" => Serpentis,
        // Drone regions
        "Cobalt Edge" | "Etherium Reach" | "The Kalevala Expanse" | "Malpais" | "Oasa"
        | "Outer Passage" | "Perrigen Falls" | "The Spire" => RogueDrones,
        // Triglavian space
        "Pochven" => Triglavian,
        _ => return None,
    };
    Some(f)
}

/// The rat profile for a region, if known.
pub fn rat_profile(region: &str) -> Option<RatProfile> {
    region_faction(region).map(profile)
}
