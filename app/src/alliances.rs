//! Alliance shorthands used in intel chat → canonical name + alliance id, so a
//! mention like "frat" or "init" can show the alliance's logo on the intel card.
//! IDs resolved from ESI (`/universe/ids/`).

/// (shorthand tokens, canonical name, alliance id). Shorthands are matched as whole,
/// lower-cased tokens.
const ALLIANCES: &[(&[&str], &str, i64)] = &[
    (&["frat", "frt", "fraternity"], "Fraternity.", 99003581),
    (&["init", "initiative"], "The Initiative.", 1900696668),
    (&["goons", "goon", "gsf"], "Goonswarm Federation", 1354830081),
    (&["horde", "phorde"], "Pandemic Horde", 99005338),
    (&["pl", "panfam"], "Pandemic Legion", 386292982),
    (&["nc"], "Northern Coalition.", 1727758877),
    (&["test", "tapi"], "Test Alliance Please Ignore", 498125261),
    (&["brave", "bni"], "Brave Collective", 99003214),
    (&["snuff", "snuffed"], "Snuffed Out", 99004901),
    (&["shadow", "scl"], "Shadow Cartel", 495729389),
    (&["bastion"], "The Bastion", 99004425),
    (&["xdeath", "xxdeathxx", "deth"], "Legion of xXDEATHXx", 1411711376),
];

/// Look up an alliance by one of its shorthands → (canonical name, id).
pub fn lookup(token: &str) -> Option<(&'static str, i64)> {
    let t = token.to_lowercase();
    ALLIANCES
        .iter()
        .find(|(sh, _, _)| sh.iter().any(|s| *s == t))
        .map(|(_, name, id)| (*name, *id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_shorthands() {
        assert_eq!(lookup("frat"), Some(("Fraternity.", 99003581)));
        assert_eq!(lookup("INIT"), Some(("The Initiative.", 1900696668)));
        assert_eq!(lookup("nobody"), None);
    }
}
