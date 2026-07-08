use std::collections::HashMap;

/// Abbreviations that map to more than one real hull. These are intentionally left unresolved and
/// surfaced as an informational badge (the candidates) rather than guessed. Keys are lowercase.
pub const AMBIGUOUS: &[(&str, &[&str])] =
    &[("sfi", &["Scythe Fleet Issue", "Stabber Fleet Issue"])];

/// The candidate hull names for an ambiguous abbreviation, if `token` is one.
pub fn ambiguous_candidates(token: &str) -> Option<&'static [&'static str]> {
    let lc = token.to_lowercase();
    AMBIGUOUS.iter().find(|(k, _)| *k == lc).map(|(_, v)| *v)
}

const NICKNAMES: &[(&str, &str)] = &[
    ("kiki", "Kikimora"),
    ("ceno", "Cenotaph"),
    ("cenos", "Cenotaph"),
    ("iki", "Ikitursa"),
    ("stileto", "Stiletto"),
    ("stilleto", "Stiletto"),
    ("stilletto", "Stiletto"),
    ("execuror", "Exequror"),
    ("exequoror", "Exequror"),
    ("exeq", "Exequror"),
    ("incursis", "Incursus"),
    ("cerb", "Cerberus"),
    ("orthus", "Orthrus"),
    ("retri", "Retribution"),
    ("sythe", "Scythe"),
    ("cythe", "Scythe"),
    ("ved", "Vedmak"),
    ("trasher", "Thrasher"),
    ("auguror", "Augoror"),
    ("porp", "Porpoise"),
    ("rorq", "Rorqual"),
    ("rorqs", "Rorqual"),
    ("bni", "Brutix Navy Issue"),
    ("eni", "Exequror Navy Issue"),
    // Acronym auto-gen drops "oni" as ambiguous (Omen vs Osprey Navy Issue); in common
    // usage ONI = Osprey Navy Issue.
    ("oni", "Osprey Navy Issue"),
    ("vaga", "Vagabond"),
    ("cane", "Hurricane"),
    ("nado", "Tornado"),
    ("mach", "Machariel"),
    ("male", "Malediction"),
    ("vindi", "Vindicator"),
    ("bhaal", "Bhaalgorn"),
    ("snake", "Rattlesnake"),
    ("ratte", "Rattlesnake"),
    ("scorp", "Scorpion"),
    ("phoon", "Typhoon"),
    ("mael", "Maelstrom"),
    ("baddon", "Abaddon"),
    ("geddon", "Armageddon"),
    ("apoc", "Apocalypse"),
    ("harby", "Harbinger"),
    ("myrm", "Myrmidon"),
    ("domi", "Dominix"),
    ("mega", "Megathron"),
    ("basi", "Basilisk"),
    ("scimi", "Scimitar"),
    ("guard", "Guardian"),
    ("lach", "Lachesis"),
    ("dram", "Dramiel"),
    ("sac", "Sacrilege"),
    ("zealot", "Zealot"),
    ("deimos", "Deimos"),
    ("ishtar", "Ishtar"),
    ("muninn", "Muninn"),
    ("eagle", "Eagle"),
    ("gila", "Gila"),
    ("worm", "Worm"),
    ("garmur", "Garmur"),
    ("cyna", "Cynabal"),
    ("cynabal", "Cynabal"),
    ("ashimmu", "Ashimmu"),
    ("nestor", "Nestor"),
    ("praxis", "Praxis"),
    ("svipul", "Svipul"),
    ("jackdaw", "Jackdaw"),
    ("confessor", "Confessor"),
    ("hecate", "Hecate"),
    ("nightmare", "Nightmare"),
    ("nm", "Nightmare"),
    ("rev", "Revelation"),
    ("phoenix", "Phoenix"),
    ("naglfar", "Naglfar"),
    ("moros", "Moros"),
    ("ferox", "Ferox"),
    ("naga", "Naga"),
    ("talos", "Talos"),
    ("oracle", "Oracle"),
    ("sabre", "Sabre"),
    ("flycatcher", "Flycatcher"),
    ("fly catcher", "Flycatcher"),
    ("heretic", "Heretic"),
    ("eris", "Eris"),
    ("broadsword", "Broadsword"),
    ("onyx", "Onyx"),
    ("phobos", "Phobos"),
    ("devoter", "Devoter"),
    ("oneiros", "Oneiros"),
    ("huginn", "Huginn"),
    ("rapier", "Rapier"),
    ("arazu", "Arazu"),
    ("curse", "Curse"),
    ("pilgrim", "Pilgrim"),
    ("falcon", "Falcon"),
    ("rook", "Rook"),
    ("blackbird", "Blackbird"),
    ("proteus", "Proteus"),
    ("loki", "Loki"),
    ("tengu", "Tengu"),
    ("legion", "Legion"),
    ("daredevil", "Daredevil"),
    ("dd", "Daredevil"),
];

pub fn aliases(by_name: &HashMap<String, (i64, String)>) -> Vec<(String, (i64, String))> {
    let mut out: Vec<(String, (i64, String))> = Vec::new();

    for (slug, canonical) in NICKNAMES {
        if let Some(entry) = by_name.get(&canonical.to_lowercase()) {
            out.push((slug.to_string(), entry.clone()));
        }
    }

    let mut acro: HashMap<String, Option<(i64, String)>> = HashMap::new();
    for (lname, entry) in by_name {
        let words: Vec<&str> = lname.split_whitespace().collect();
        if words.len() < 2 {
            continue;
        }
        let a: String = words.iter().filter_map(|w| w.chars().next()).collect();
        // Require >= 3 letters — 2-letter acronyms collide with common words
        // ("is" = InterBus Shuttle, matching the English word "is").
        if a.len() < 3 {
            continue;
        }
        acro.entry(a)
            .and_modify(|e| *e = None)
            .or_insert_with(|| Some(entry.clone()));
    }
    for (a, entry) in acro {
        if let Some(entry) = entry {
            if !by_name.contains_key(&a) {
                out.push((a, entry));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nicknames_and_acronyms_resolve() {
        let by_name: HashMap<String, (i64, String)> = [
            (1i64, "Vagabond"),
            (2, "Hurricane"),
            (3, "Cyclone Fleet Issue"),
            (4, "Raven Navy Issue"),
            (5, "Flycatcher"),
        ]
        .into_iter()
        .map(|(id, n)| (n.to_lowercase(), (id, n.to_string())))
        .collect();
        let map: HashMap<String, (i64, String)> = aliases(&by_name).into_iter().collect();
        assert_eq!(map.get("vaga").map(|e| e.0), Some(1));
        assert_eq!(map.get("cane").map(|e| e.0), Some(2));
        assert_eq!(map.get("cfi").map(|e| e.0), Some(3));
        assert_eq!(map.get("rni").map(|e| e.0), Some(4));
        assert_eq!(map.get("fly catcher").map(|e| e.0), Some(5));
    }

    #[test]
    fn ved_resolves_to_vedmak() {
        let by_name: HashMap<String, (i64, String)> = [(1i64, "Vedmak")]
            .into_iter()
            .map(|(id, n)| (n.to_lowercase(), (id, n.to_string())))
            .collect();
        let map: HashMap<String, (i64, String)> = aliases(&by_name).into_iter().collect();
        assert_eq!(map.get("ved").map(|e| e.1.as_str()), Some("Vedmak"));
    }

    #[test]
    fn edit_distance_works() {
        assert_eq!(edit_distance("drake", "drake"), 0);
        assert_eq!(edit_distance("vagabon", "vagabond"), 1);
    }
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // Optimal string alignment: like Levenshtein but an adjacent transposition ("saber" vs
    // "sabre") costs 1, not 2 — the most common keyboard typo. Needs the row two back.
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            let mut v = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
            if i > 0 && j > 0 && ca == b[j - 1] && a[i - 1] == cb {
                v = v.min(prev2[j - 1] + 1);
            }
            cur[j + 1] = v;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
