//! The colours the client offers wherever it lets a pilot or a state be tinted: overview colour
//! tags and backgrounds, and the fleet watch list.

pub const GAME: &[(&str, [f32; 3])] = &[
    (tr_noop!("Red"), [0.75, 0.0, 0.0]),
    (tr_noop!("Orange"), [1.0, 0.35, 0.0]),
    (tr_noop!("Yellow"), [1.0, 0.7, 0.0]),
    (tr_noop!("Green"), [0.1, 0.6, 0.1]),
    (tr_noop!("Turquoise"), [0.0, 0.63, 0.57]),
    (tr_noop!("Dark turquoise"), [0.0, 0.34, 0.33]),
    (tr_noop!("Blue"), [0.2, 0.5, 1.0]),
    (tr_noop!("Dark blue"), [0.0, 0.15, 0.6]),
    (tr_noop!("Indigo"), [0.3, 0.0, 0.5]),
    (tr_noop!("Purple"), [0.6, 0.15, 0.9]),
    (tr_noop!("White"), [0.7, 0.7, 0.7]),
    (tr_noop!("Black"), [0.0, 0.0, 0.0]),
];

pub fn same(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.005)
}

/// The name of a colour from the game's own set, or `None` for any other.
pub fn name_of(c: [f32; 3]) -> Option<&'static str> {
    GAME.iter().find(|(_, g)| same(*g, c)).map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colours_seen_in_settings_files_are_named() {
        // Every watch list and state colour met in real files so far.
        for c in [[0.0, 0.15, 0.6], [0.1, 0.6, 0.1], [0.75, 0.0, 0.0], [1.0, 0.35, 0.0], [1.0, 0.7, 0.0], [0.0, 0.34, 0.33], [0.0, 0.63, 0.57], [0.2, 0.5, 1.0], [0.7, 0.7, 0.7]] {
            assert!(name_of(c).is_some(), "{c:?}");
        }
        assert_eq!(name_of([0.75, 0.0, 0.0]), Some("Red"));
        assert_eq!(name_of([0.5, 0.5, 0.1]), None);
        let mut seen = std::collections::BTreeSet::new();
        assert!(GAME.iter().all(|(n, _)| seen.insert(*n)));
    }
}
