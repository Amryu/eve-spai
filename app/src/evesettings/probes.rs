//! Custom scan probe formations, stored per account.

use super::file::SettingsFile;
use super::marshal::Node;

const KEY: &str = "probescanning.customFormations";
const SELECTED: &str = "probescanning.selectedFormationID";

pub const AU: f64 = 149_597_870_700.0;
pub const KM: f64 = 1_000.0;
/// What a launcher holds, and so the most a formation can place.
pub const MAX_PROBES: usize = 8;
/// The scan ranges a probe can be set to, in AU.
pub const RANGES_AU: &[f64] = &[0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    /// Metres from the formation's centre.
    pub pos: [f64; 3],
    /// Scan range in metres.
    pub range: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Formation {
    pub id: i64,
    pub name: String,
    pub probes: Vec<Probe>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Formations {
    pub list: Vec<Formation>,
    pub selected: Option<i64>,
}

impl Formation {
    /// Distance of the farthest probe from the centre, in metres.
    pub fn extent(&self) -> f64 {
        self.probes.iter().map(|p| length(p.pos)).fold(0.0, f64::max)
    }
}

pub fn length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

impl Formations {
    pub fn next_id(&self) -> i64 {
        self.list.iter().map(|f| f.id).max().map_or(0, |m| m.max(-1) + 1)
    }
}

pub fn read(account: &SettingsFile) -> Formations {
    let mut out = Formations::default();
    if let Some(dict) = account.get("ui", KEY) {
        for (k, v) in dict.pairs() {
            let parts = v.items();
            let (Some(id), [name, probes]) = (k.int(), parts.as_slice()) else {
                continue;
            };
            let probes = probes
                .items()
                .iter()
                .filter_map(|p| {
                    let parts = p.items();
                    let [pos, range] = parts.as_slice() else {
                        return None;
                    };
                    let xyz: Vec<f64> = pos.items().iter().filter_map(|c| c.num()).collect();
                    Some(Probe { pos: [*xyz.first()?, *xyz.get(1)?, *xyz.get(2)?], range: range.num()? })
                })
                .collect();
            out.list.push(Formation { id, name: name.text().unwrap_or_default().into_owned(), probes });
        }
    }
    out.list.sort_by_key(|f| f.id);
    out.selected = account.get("ui", SELECTED).and_then(|v| v.int());
    out
}

pub fn write(account: &mut SettingsFile, formations: &Formations) {
    let pairs = formations
        .list
        .iter()
        .map(|f| {
            let probes = f
                .probes
                .iter()
                .map(|p| {
                    Node::tuple(vec![
                        Node::tuple(p.pos.iter().map(|c| Node::float(*c)).collect()),
                        Node::float(p.range),
                    ])
                })
                .collect();
            (Node::int(f.id), Node::tuple(vec![Node::text(&f.name), Node::list(probes)]))
        })
        .collect();
    account.set("ui", KEY, Node::dict(pairs));
    if let Some(id) = formations.selected.filter(|id| formations.list.iter().any(|f| f.id == *id)) {
        account.set("ui", SELECTED, Node::int(id));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One in the middle, six on the axes: the usual pinpoint.
    Pinpoint,
    /// One in the middle and a flat ring around it.
    Ring,
    /// The corners of a cube.
    Cube,
    /// A straight line through the centre.
    Line,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Pinpoint, Shape::Ring, Shape::Cube, Shape::Line];

    pub fn label(self) -> &'static str {
        match self {
            Shape::Pinpoint => "Pinpoint",
            Shape::Ring => "Ring",
            Shape::Cube => "Cube",
            Shape::Line => "Line",
        }
    }
}

/// `count` probes in `shape`, the outermost `radius` metres from the centre.
pub fn generate(shape: Shape, count: usize, radius: f64, range: f64) -> Vec<Probe> {
    let count = count.clamp(1, MAX_PROBES);
    let at = |pos: [f64; 3]| Probe { pos, range };
    let mut out = Vec::new();
    match shape {
        Shape::Pinpoint => {
            out.push(at([0.0; 3]));
            for axis in [0, 2, 1] {
                for sign in [1.0, -1.0] {
                    let mut pos = [0.0; 3];
                    pos[axis] = sign * radius;
                    out.push(at(pos));
                }
            }
        }
        Shape::Ring => {
            out.push(at([0.0; 3]));
            let n = count.saturating_sub(1).max(1);
            for i in 0..n {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                out.push(at([radius * a.cos(), 0.0, radius * a.sin()]));
            }
        }
        Shape::Cube => {
            let side = radius / 3f64.sqrt();
            for i in 0..8 {
                let s = |bit: u32| if i >> bit & 1 == 0 { side } else { -side };
                out.push(at([s(0), s(1), s(2)]));
            }
        }
        Shape::Line => {
            for i in 0..count {
                let t = if count == 1 { 0.0 } else { i as f64 / (count - 1) as f64 * 2.0 - 1.0 };
                out.push(at([t * radius, 0.0, 0.0]));
            }
        }
    }
    out.truncate(count);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formations_round_trip() {
        let mut f = SettingsFile::empty();
        assert_eq!(read(&f), Formations::default());
        let all = Formations {
            list: vec![
                Formation { id: -4, name: "Gate perches".into(), probes: generate(Shape::Ring, 8, 300.0 * KM, 0.25 * AU) },
                Formation { id: 2, name: "Wide ✪".into(), probes: generate(Shape::Pinpoint, 7, 4.0 * AU, 8.0 * AU) },
            ],
            selected: Some(2),
        };
        write(&mut f, &all);
        let back = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        assert_eq!(read(&back), all);
        assert_eq!(all.next_id(), 3);
        assert_eq!(Formations::default().next_id(), 0);
    }

    #[test]
    fn shapes_keep_probes_within_the_radius() {
        for shape in Shape::ALL {
            for count in 1..=MAX_PROBES {
                let probes = generate(shape, count, 1000.0, AU);
                assert_eq!(probes.len(), count.min(match shape {
                    Shape::Pinpoint => 7,
                    _ => 8,
                }), "{shape:?} {count}");
                assert!(probes.iter().all(|p| length(p.pos) <= 1000.0 + 1e-6 && p.range == AU));
            }
        }
        let cube = generate(Shape::Cube, 8, 1000.0, AU);
        assert!((Formation { id: 0, name: String::new(), probes: cube }.extent() - 1000.0).abs() < 1e-6);
    }
}
