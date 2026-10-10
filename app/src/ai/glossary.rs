//! EVE terms the assistant should read the way pilots mean them. The base list ships with the app;
//! the user can change any entry (and put it back), hide one, or add their own.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const BASE: &[(&str, &str)] = &[
    ("ADM", "Activity Defense Multiplier: how hard a sov system is to attack, raised by ratting, mining and holding it"),
    ("Ansiblex", "A player-built jump bridge between two systems in the same alliance's space"),
    ("AFK cloaky", "A cloaked pilot sitting in a system, usually to disrupt ratting or to light a cyno later"),
    ("Anom", "Cosmic anomaly: a site found without probes, mostly ratting and mining"),
    ("Blops", "Black Ops battleships, which bridge stealth bombers and recons onto a covert cyno"),
    ("Blue", "A pilot or group with positive standing: a friend"),
    ("Bubble", "A warp disruption field that stops ships from warping; dragged by interdictors or anchored"),
    ("Bridge", "Moving a fleet by a titan or Black Ops jump portal, or using an Ansiblex"),
    ("Camp", "Hostiles waiting on a gate or station to catch passing ships"),
    ("Cap", "A capital ship: carrier, dreadnought, force auxiliary, supercarrier or titan; also the capacitor"),
    ("Clear", "No hostiles in local or on scan in the system named"),
    ("Coord", "The fleet coordinator, who runs the rescue or the response for the FCs"),
    ("Covert cyno", "A cyno only Black Ops and covert ships can jump to; it does not show on the overview"),
    ("Cyno", "A cynosural field, the beacon capitals jump to; 'cyno up' means one is lit"),
    ("D-scan", "Directional scanner: shows ships and structures within 14 AU in a cone"),
    ("Dic", "Interdictor: a destroyer that launches warp disruption bubbles"),
    ("Doctrine", "The set of ships and fittings a fleet flies"),
    ("Drop", "Capitals or Black Ops jumping in on a cyno onto a target"),
    ("Dropper", "A ship lighting a cyno to bring capitals in"),
    ("Drifter", "Hostile NPC battleships; also the wormholes they use, which link to Jove observatory systems"),
    ("ESS", "Encounter Surveillance System: a structure that pools bounty payouts and can be robbed"),
    ("FC", "Fleet commander"),
    ("Frat", "Fraternity., a large alliance"),
    ("Gank", "Killing a ship, often an unprepared one, with overwhelming force"),
    ("Gate", "A stargate between two systems"),
    ("Hic", "Heavy interdictor: a cruiser with an infinite point or a bubble that stops capitals"),
    ("Hot drop", "Capitals or Black Ops arriving suddenly through a cyno"),
    ("Hostile", "A pilot with negative or no standing, treated as an enemy"),
    ("Hunter", "A pilot roaming to find and kill ratters or miners"),
    ("tek", "Said for the dash in a system name: four tek H is 4-H, one D Q one tek A is 1DQ1-A; a partial name like 4-H is the start of one, so search for it"),
    ("Init", "The Initiative., a nullsec alliance known for black ops (blops) drops and stealth bomber gangs; voice may hear it as innit"),
    ("Intel", "Reports of hostile movements posted in shared intel channels"),
    ("Jove observatory", "A structure in some systems where drifter wormholes appear"),
    ("Jump", "One gate jump between systems; distances are counted in jumps"),
    ("JB", "Jump bridge, usually an Ansiblex"),
    ("Kite", "Fighting at range while staying out of the enemy's reach"),
    ("Local", "The local chat channel, which lists everyone in the system in known space"),
    ("Logi", "Logistics ships that repair other fleet members"),
    ("LY", "Light years: the distance capitals and Black Ops can jump"),
    ("Neut", "A neutral pilot (no standing); also a capacitor neutralizer"),
    ("No vis", "The reporter cannot see the hostile on grid or scan"),
    ("Nullsec", "Space with security 0.0 or below, held by player alliances"),
    ("Op", "A fleet operation"),
    ("Pochven", "Triglavian-held space reached by filaments and wormholes"),
    ("Pod", "The capsule a pilot sits in; 'podded' means the capsule was killed"),
    ("Point", "A warp scrambler or disruptor holding a ship in place"),
    ("Ratting", "Killing NPC pirates for bounties"),
    ("Red", "A pilot or group with negative standing: an enemy"),
    ("Roam", "A small gang moving through space looking for fights"),
    ("Sabre", "A popular interdictor"),
    ("Sig", "A cosmic signature that needs probing: wormholes, data and relic sites"),
    ("Skyhook", "An orbital structure on planets that produces resources and can be raided"),
    ("Smartbomb", "A module damaging everything around the ship, used on gate camps"),
    ("Sov", "Sovereignty: which alliance holds a nullsec system"),
    ("Spike", "A sudden rise in the number of pilots in local"),
    ("Stage, staging", "The system a group bases its fleets from"),
    ("Super", "A supercarrier"),
    ("Tackle", "Holding a target in place with points, webs or bubbles; 'tackled' means caught"),
    ("Thera", "A wormhole hub system with many connections to known space"),
    ("Titan", "The largest capital, able to bridge fleets"),
    ("Turnur", "A low-sec system with a permanent wormhole hub"),
    ("Undock", "Leaving a station"),
    ("Web", "A stasis webifier that slows a target"),
    ("WH", "Wormhole: an unstable connection between systems, found by probing"),
    ("x", "After a number: that many ships; '+5' or 'x5' means five more"),
    ("Zarzakh", "A system with special jump rules reached from several regions"),
    ("zKill", "zKillboard, the public killmail database"),
];

/// The user's changes to the glossary.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edits {
    /// Base entries with their own meaning, by term. An empty meaning hides the entry.
    pub overrides: BTreeMap<String, String>,
    /// The user's own entries.
    pub custom: Vec<Entry>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub term: String,
    pub meaning: String,
}

/// One row of the glossary as it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub term: String,
    pub meaning: String,
    pub base: Option<&'static str>,
    pub custom: bool,
}

impl Row {
    pub fn changed(&self) -> bool {
        self.base.is_some_and(|b| b != self.meaning)
    }
}

pub fn rows(e: &Edits) -> Vec<Row> {
    let mut out: Vec<Row> = BASE
        .iter()
        .map(|(t, m)| Row {
            term: (*t).to_owned(),
            meaning: e.overrides.get(*t).cloned().unwrap_or_else(|| (*m).to_owned()),
            base: Some(m),
            custom: false,
        })
        .collect();
    out.extend(e.custom.iter().filter(|c| !c.term.trim().is_empty()).map(|c| Row { term: c.term.clone(), meaning: c.meaning.clone(), base: None, custom: true }));
    out
}

/// The glossary as the model reads it: hidden entries left out.
pub fn prompt(e: &Edits) -> String {
    let mut s = String::from("EVE terms as pilots use them:\n");
    for r in rows(e).iter().filter(|r| !r.meaning.trim().is_empty()) {
        s.push_str(&format!("- {}: {}\n", r.term, r.meaning.trim()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_override_hide_and_add() {
        let mut e = Edits::default();
        assert!(prompt(&e).contains("- Cyno: A cynosural field"));
        e.overrides.insert("Cyno".into(), "a beacon".into());
        e.overrides.insert("Frat".into(), String::new());
        e.custom.push(Entry { term: "GSOL".into(), meaning: "a holding corp".into() });
        let p = prompt(&e);
        assert!(p.contains("- Cyno: a beacon"));
        assert!(!p.contains("- Frat:"), "hidden");
        assert!(p.contains("- GSOL: a holding corp"));
        let r = rows(&e);
        assert!(r.iter().find(|x| x.term == "Cyno").unwrap().changed());
        assert!(!r.iter().find(|x| x.term == "Pod").unwrap().changed());
    }
}
