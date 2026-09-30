//! Fixtures for tests here and in the app.

use std::collections::HashMap;

use crate::geo::{SystemInfo, Systems};

/// 1DQ1-A, 319-3D and 7-K5EL gate-linked in a row, with Jita, Thera and a J-space system apart,
/// plus `extra` systems with no gates: enough of New Eden for routing and map tests.
pub fn small_universe(extra: &[(i64, String, f64, String)]) -> Systems {
    let mut by_name = HashMap::new();
    let extra = extra.iter().map(|(id, n, s, r)| (*id, n.as_str(), *s, r.as_str()));
    for (id, name, security, region) in [
        (30_004_759_i64, "1DQ1-A", -0.36_f64, "Delve"),
        (30_004_608, "319-3D", -0.41, "Delve"),
        (30_003_704, "7-K5EL", -0.29, "Fountain"),
        (30_000_142, "Jita", 0.95, "The Forge"),
        (31_000_005, "Thera", -1.0, "G-R00031"),
        (31_000_002, "J110145", -1.0, "A-R00001"),
    ]
    .into_iter()
    .chain(extra)
    {
        by_name.insert(
            name.to_lowercase(),
            SystemInfo {
                id,
                name: name.to_owned(),
                security,
                constellation: "O-EImg".into(),
                region: region.to_owned(),
                faction: String::new(),
            },
        );
    }
    let adjacency = HashMap::from([
        (30_004_759, vec![30_004_608]),
        (30_004_608, vec![30_004_759, 30_003_704]),
        (30_003_704, vec![30_004_608]),
    ]);
    let mut s = Systems::new(by_name, adjacency);
    let ly = crate::map::LY_METERS;
    s.set_positions(HashMap::from([
        (30_004_759, [0.0, 0.0, 0.0]),
        (30_004_608, [2.1 * ly, 0.0, 0.0]),
        (30_003_704, [3.0 * ly, 0.0, 4.4 * ly]),
        (30_000_142, [-30.0 * ly, 1.5 * ly, 22.0 * ly]),
    ]));
    s
}
