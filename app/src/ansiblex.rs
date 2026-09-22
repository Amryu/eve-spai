//! Ansiblex zones and the dotlan bridge link.
//!
//! Since Cradle of War (22 Sep 2026) an Ansiblex jump costs capacitor, scaled by the zone of the
//! destination gate: its distance from the owning alliance's capital system. Zone 1 (0-5 LY) is
//! free, then x2, x6, x9 and x15 per 5 LY band. Gate-to-gate distance does not count, so the two
//! directions of one bridge can sit in different zones.

use crate::geo::Systems;
use crate::settings::JumpBridge;

pub const DEFAULT_CAPITAL: &str = "A24L-V";
pub const DEFAULT_MAX_ZONE: u8 = 2;
pub const MAX_ZONE: u8 = 5;

pub fn zone_for_ly(ly: f64) -> u8 {
    match ly {
        l if l <= 5.0 => 1,
        l if l <= 10.0 => 2,
        l if l <= 15.0 => 3,
        l if l <= 20.0 => 4,
        _ => 5,
    }
}

pub fn zone_label(zone: u8) -> String {
    match zone {
        1 => "Zone 1 (free)".to_owned(),
        z => format!("Zone {z}"),
    }
}

/// One bridge with the zone each direction lands in. `None` when the capital or a position is
/// unknown, which [`Bridge::usable`] treats as usable: a typo in the capital must not quietly
/// strip every bridge from every route.
#[derive(Clone, Debug, PartialEq)]
pub struct Bridge {
    pub a: i64,
    pub b: i64,
    pub zone_at_b: Option<u8>,
    pub zone_at_a: Option<u8>,
}

impl Bridge {
    pub fn usable(zone: Option<u8>, max_zone: u8) -> bool {
        zone.is_none_or(|z| z <= max_zone)
    }

    pub fn forward(&self, max_zone: u8) -> bool {
        Self::usable(self.zone_at_b, max_zone)
    }

    pub fn back(&self, max_zone: u8) -> bool {
        Self::usable(self.zone_at_a, max_zone)
    }
}

pub fn bridges(list: &[JumpBridge], systems: &Systems, capital: &str) -> Vec<Bridge> {
    let cap = systems.lookup(capital.trim()).map(|i| i.id);
    let zone = |dest: i64| cap.and_then(|c| systems.ly_between(c, dest)).map(zone_for_ly);
    list.iter()
        .filter_map(|b| {
            let a = systems.lookup(&b.from)?.id;
            let b = systems.lookup(&b.to)?.id;
            Some(Bridge { a, b, zone_at_b: zone(b), zone_at_a: zone(a) })
        })
        .collect()
}

/// The directed edges routing may use: each direction whose destination is within `max_zone`.
pub fn permitted_edges(list: &[JumpBridge], systems: &Systems, capital: &str, max_zone: u8) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for br in bridges(list, systems, capital) {
        if br.forward(max_zone) {
            out.push((br.a, br.b));
        }
        if br.back(max_zone) {
            out.push((br.b, br.a));
        }
    }
    out
}

/// Everything that decides the bridge edges in the graph, so a change to any of it rebuilds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BridgeKey {
    pub bridges: Vec<JumpBridge>,
    pub capital: String,
    pub max_zone: u8,
}

impl BridgeKey {
    pub fn of(s: &crate::settings::Settings) -> Self {
        Self {
            bridges: s.jump_bridges.clone(),
            capital: s.ansiblex_capital.clone(),
            max_zone: s.ansiblex_max_zone,
        }
    }
}

/// Lays the bridge directions the zone limit permits over a freshly loaded graph.
pub fn feed(settings: &crate::settings::Settings, systems: &mut Systems) -> BridgeKey {
    let key = BridgeKey::of(settings);
    let edges = permitted_edges(&key.bridges, systems, &key.capital, key.max_zone);
    systems.add_directed_bridges(&edges);
    key
}

/// The name pairs of a dotlan bridge link, `https://evemaps.dotlan.net/universe/A::B,C::D`, or
/// `None` when `text` holds no such link. Names come back as written, unresolved.
pub fn dotlan_pairs(text: &str) -> Option<Vec<(String, String)>> {
    let at = text.find("dotlan.net/")?;
    let rest = &text[at + "dotlan.net/".len()..];
    let url = rest.split(char::is_whitespace).next().unwrap_or_default();
    let url = url.split(['?', '#']).next().unwrap_or_default();
    let segment = url.split('/').find(|seg| seg.contains("::"))?;
    let pairs = segment
        .split(',')
        .filter_map(|item| {
            let (a, b) = item.split_once("::")?;
            let (a, b) = (decode_name(a), decode_name(b));
            (!a.is_empty() && !b.is_empty()).then_some((a, b))
        })
        .collect();
    Some(pairs)
}

fn decode_name(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'_' | b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::SystemInfo;
    use std::collections::HashMap;

    const LY: f64 = crate::map::LY_METERS;

    /// Capital at the origin, one system per zone band along x, all gated in a line.
    fn graph() -> Systems {
        let names = [("CAP", 1, 0.0), ("NEAR", 2, 4.0), ("MID", 3, 7.0), ("FAR", 4, 12.0), ("OLD MAN STAR", 5, 30.0)];
        let by_name = names
            .iter()
            .map(|&(n, id, _)| {
                let info = SystemInfo {
                    id,
                    name: n.to_owned(),
                    security: 0.0,
                    constellation: String::new(),
                    region: String::new(),
                    faction: String::new(),
                };
                (n.to_lowercase(), info)
            })
            .collect();
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        for (a, b) in [(1, 2), (2, 3), (3, 4), (4, 5)] {
            adjacency.entry(a).or_default().push(b);
            adjacency.entry(b).or_default().push(a);
        }
        let mut s = Systems::new(by_name, adjacency);
        s.set_positions(names.iter().map(|&(_, id, x)| (id, [x * LY, 0.0, 0.0])).collect());
        s
    }

    fn jb(from: &str, to: &str) -> JumpBridge {
        JumpBridge { from: from.to_owned(), to: to.to_owned() }
    }

    #[test]
    fn zone_bands_follow_the_patch_notes() {
        assert_eq!(zone_for_ly(0.0), 1);
        assert_eq!(zone_for_ly(5.0), 1);
        assert_eq!(zone_for_ly(5.1), 2);
        assert_eq!(zone_for_ly(10.0), 2);
        assert_eq!(zone_for_ly(10.1), 3);
        assert_eq!(zone_for_ly(20.0), 4);
        assert_eq!(zone_for_ly(20.1), 5);
    }

    #[test]
    fn each_direction_is_priced_by_where_it_lands() {
        let g = graph();
        let br = bridges(&[jb("NEAR", "FAR")], &g, "CAP");
        assert_eq!(br[0].zone_at_b, Some(3), "FAR sits 12 LY from the capital");
        assert_eq!(br[0].zone_at_a, Some(1));
        assert_eq!(permitted_edges(&[jb("NEAR", "FAR")], &g, "CAP", 2), vec![(4, 2)], "only the way home");
        assert_eq!(permitted_edges(&[jb("NEAR", "FAR")], &g, "CAP", 3), vec![(2, 4), (4, 2)]);
        assert!(permitted_edges(&[jb("FAR", "OLD MAN STAR")], &g, "CAP", 2).is_empty());
    }

    #[test]
    fn an_unknown_capital_keeps_every_bridge() {
        let g = graph();
        let edges = permitted_edges(&[jb("FAR", "OLD MAN STAR")], &g, "NOWHERE", 1);
        assert_eq!(edges, vec![(4, 5), (5, 4)]);
    }

    #[test]
    fn the_graph_only_gets_the_permitted_direction() {
        let mut g = graph();
        let edges = permitted_edges(&[jb("CAP", "FAR")], &g, "CAP", 1);
        g.add_directed_bridges(&edges);
        assert_eq!(g.jumps(4, 1, 10), Some(1), "back to the capital is zone 1");
        assert_eq!(g.jumps(1, 4, 10), Some(3), "out to FAR is zone 3, so it goes by gates");
        let to_far = g.distances_to(4, true, true, &HashMap::new(), |_| true);
        assert_eq!(to_far[&1], 3);
        let to_cap = g.distances_to(1, true, true, &HashMap::new(), |_| true);
        assert_eq!(to_cap[&4], 1);
        assert_eq!(g.nearest_matching(4, 10, |id| id == 1).unwrap().0, 3);
        assert_eq!(g.nearest_matching(1, 10, |id| id == 4).unwrap().0, 1);
        assert_eq!(g.route(1, 4, true, true, |_| true).map(|r| r.len()), Some(4));
    }

    #[test]
    fn a_dotlan_link_reads_as_pairs() {
        let link = "https://evemaps.dotlan.net/universe/PQRE-W::A-7XFN,H-FGJO::G3D-ZT,C5-SUU::MWA-5Q";
        let pairs = dotlan_pairs(link).unwrap();
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0], ("PQRE-W".to_owned(), "A-7XFN".to_owned()));
        assert_eq!(pairs[2], ("C5-SUU".to_owned(), "MWA-5Q".to_owned()));
    }

    #[test]
    fn a_dotlan_link_decodes_spaces_and_ignores_its_tail() {
        let pairs = dotlan_pairs("see https://evemaps.dotlan.net/universe/Old_Man_Star::A%20B,X::?foo=1 ok").unwrap();
        assert_eq!(pairs, vec![("Old Man Star".to_owned(), "A B".to_owned())]);
        assert_eq!(dotlan_pairs("1DQ1-A » O-EIMK"), None);
    }
}
