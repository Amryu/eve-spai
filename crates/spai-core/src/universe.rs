//! New Eden as one file, for an app with no database: systems, their gates, regions and positions,
//! from EVE's static data. `spai-assets` writes it from the desktop's imported tables; the web app
//! loads it into the same graph the desktop builds.

use std::collections::HashMap;
use std::io::Read as _;

use serde::{Deserialize, Serialize};

use crate::geo::{SystemInfo, Systems};
use crate::map::MapSystem;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Universe {
    pub regions: Vec<(i64, String)>,
    pub systems: Vec<System>,
    /// Stargate links, both directions listed.
    pub jumps: Vec<(i64, i64)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct System {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub region_id: i64,
    pub constellation: String,
    pub faction: String,
    /// Metres, as the static data has them.
    pub pos: [f64; 3],
    /// Where the map draws it.
    pub pos2d: [f64; 2],
}

impl Universe {
    pub fn to_gz(&self) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        serde_json::to_writer(&mut enc, self).expect("serializes");
        enc.finish().expect("in memory")
    }

    pub fn from_gz(bytes: &[u8]) -> Result<Self, String> {
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(bytes).read_to_end(&mut json).map_err(|e| format!("universe: {e}"))?;
        serde_json::from_slice(&json).map_err(|e| format!("universe: {e}"))
    }

    /// The routing graph, as the desktop builds it from its database.
    pub fn systems(&self) -> Systems {
        let regions: HashMap<i64, String> = self.regions.iter().cloned().collect();
        let by_name = self
            .systems
            .iter()
            .map(|s| {
                let info = SystemInfo {
                    id: s.id,
                    name: s.name.clone(),
                    security: s.security,
                    constellation: s.constellation.clone(),
                    region: regions.get(&s.region_id).cloned().unwrap_or_default(),
                    faction: s.faction.clone(),
                };
                (s.name.to_lowercase(), info)
            })
            .collect();
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        for (a, b) in &self.jumps {
            adjacency.entry(*a).or_default().push(*b);
        }
        let mut g = Systems::new(by_name, adjacency);
        g.set_positions(self.systems.iter().map(|s| (s.id, s.pos)).collect());
        g
    }

    /// Every system as the star map draws it.
    pub fn map_systems(&self) -> Vec<MapSystem> {
        self.systems
            .iter()
            .map(|s| MapSystem {
                id: s.id,
                name: s.name.clone(),
                security: s.security,
                region_id: s.region_id,
                x: s.pos[0],
                y: s.pos[1],
                z: s.pos[2],
                x2d: s.pos2d[0],
                z2d: s.pos2d[1],
            })
            .collect()
    }

    pub fn region_name(&self, id: i64) -> String {
        self.regions.iter().find(|(r, _)| *r == id).map(|(_, n)| n.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_universe_survives_its_file_and_routes() {
        let sys = |id, name: &str, x| System {
            id,
            name: name.into(),
            security: 0.9,
            region_id: 1,
            constellation: "C".into(),
            faction: String::new(),
            pos: [x, 0.0, 0.0],
            pos2d: [x, 0.0],
        };
        let u = Universe { regions: vec![(1, "The Forge".into())], systems: vec![sys(1, "Jita", 0.0), sys(2, "Perimeter", 1e16)], jumps: vec![(1, 2), (2, 1)] };
        let back = Universe::from_gz(&u.to_gz()).unwrap();
        assert_eq!(back, u);
        let g = back.systems();
        assert_eq!(g.lookup("jita").map(|i| i.region.as_str()), Some("The Forge"));
        assert_eq!(g.jumps(1, 2, 5), Some(1));
        assert_eq!(back.map_systems()[1].x, 1e16);
    }
}
