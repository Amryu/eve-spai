use emath::{Pos2, Rect, Vec2};

/// A system as the map draws it: 3D position for distances, the flattened 2D one for drawing.
#[derive(Clone, Debug)]
pub struct MapSystem {
    pub id: i64,
    pub name: String,
    pub security: f64,
    pub region_id: i64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub x2d: f64,
    pub z2d: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapView {
    Universe,
    Region(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MapLayout {
    Geographic,
    Spaced,
    Radial,
    Tree,
}

impl Default for MapLayout {
    fn default() -> Self {
        MapLayout::Spaced
    }
}

impl MapLayout {
    pub fn is_threat(self) -> bool {
        matches!(self, MapLayout::Radial | MapLayout::Tree)
    }
}

pub struct Bounds {
    min_x: f64,
    max_x: f64,
    min_z: f64,
    max_z: f64,
}

impl Bounds {
    pub fn of(systems: &[MapSystem]) -> Option<Bounds> {
        let first = systems.first()?;
        let mut b = Bounds {
            min_x: first.x,
            max_x: first.x,
            min_z: first.z,
            max_z: first.z,
        };
        for s in systems {
            b.min_x = b.min_x.min(s.x);
            b.max_x = b.max_x.max(s.x);
            b.min_z = b.min_z.min(s.z);
            b.max_z = b.max_z.max(s.z);
        }
        Some(b)
    }

    fn mid_x(&self) -> f64 {
        (self.min_x + self.max_x) / 2.0
    }
    fn mid_z(&self) -> f64 {
        (self.min_z + self.max_z) / 2.0
    }

    pub fn base_scale(&self, rect: Rect, margin: f32) -> f32 {
        let w = (rect.width() - 2.0 * margin).max(1.0) as f64;
        let h = (rect.height() - 2.0 * margin).max(1.0) as f64;
        let span_x = (self.max_x - self.min_x).max(1.0);
        let span_z = (self.max_z - self.min_z).max(1.0);
        (w / span_x).min(h / span_z) as f32
    }
}

/// The light year as EVE counts it for jump range and Ansiblex zones, rounded from the true
/// 9.4607e15 m. 2-Q4YG is 4.9998 true light years from A24L-V and Zone 2 in game, which only this
/// figure (5.0002) gives.
pub const LY_METERS: f64 = 9.46e15;

/// Max jump-drive ranges (light-years) at maxed skills (Jump Drive Calibration V, +100%).
/// Values are the live SDE `jumpDriveRange` doubled: titan/super 3.0, other capitals 3.5,
/// black ops 4.0, jump freighter and rorqual 5.0.
pub const JUMP_RANGES: &[(&str, f64)] = &[
    ("Super / Titan", 6.0),
    ("Capital", 7.0),
    ("Black Ops", 8.0),
    ("Jump Freighter", 10.0),
];

pub fn ly_distance(a: &MapSystem, b: &MapSystem) -> f64 {
    let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
    d / LY_METERS
}

pub fn ly_to_pixels(ly: f64, b: &Bounds, rect: Rect, zoom: f32) -> f32 {
    (ly * LY_METERS) as f32 * b.base_scale(rect, 30.0) * zoom
}

/// How far apart neighbouring systems usually sit, in map units: the median distance from each
/// system to its nearest one. Sizes on the map follow this times the scale, so they look the same
/// whatever the window, display scaling or layout.
pub fn typical_spacing(systems: &[MapSystem]) -> f64 {
    let Some(b) = Bounds::of(systems) else { return 0.0 };
    let span = (b.max_x - b.min_x).max(b.max_z - b.min_z).max(1.0);
    let cell = span / (systems.len() as f64).sqrt().max(1.0);
    let key = |s: &MapSystem| (((s.x - b.min_x) / cell) as i64, ((s.z - b.min_z) / cell) as i64);
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    for (i, s) in systems.iter().enumerate() {
        grid.entry(key(s)).or_default().push(i);
    }
    let mut nearest: Vec<f64> = Vec::with_capacity(systems.len());
    for (i, s) in systems.iter().enumerate() {
        let (cx, cz) = key(s);
        let mut best = f64::INFINITY;
        // Rings outward until one holds a neighbour, then one ring more: the nearest can sit just
        // across a cell edge.
        let mut found_at = None;
        for r in 0..64i64 {
            if found_at.is_some_and(|f| r > f + 1) {
                break;
            }
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    for &j in grid.get(&(cx + dx, cz + dz)).into_iter().flatten() {
                        if j != i {
                            best = best.min((systems[j].x - s.x).hypot(systems[j].z - s.z));
                        }
                    }
                }
            }
            if best.is_finite() && found_at.is_none() {
                found_at = Some(r);
            }
        }
        if best.is_finite() {
            nearest.push(best);
        }
    }
    if nearest.is_empty() {
        return 0.0;
    }
    let mid = nearest.len() / 2;
    *nearest.select_nth_unstable_by(mid, |a, b| a.total_cmp(b)).1
}

pub fn project(x: f64, z: f64, b: &Bounds, rect: Rect, zoom: f32, pan: Vec2) -> Pos2 {
    let scale = b.base_scale(rect, 30.0) * zoom;
    let center = rect.center() + pan;
    Pos2::new(
        center.x + ((x - b.mid_x()) as f32) * scale,
        center.y - ((z - b.mid_z()) as f32) * scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_is_the_usual_gap_to_the_nearest_system() {
        // A 10-unit grid with one far outlier: the outlier must not stretch the answer.
        let mut grid: Vec<MapSystem> = (0..20).flat_map(|i| (0..20).map(move |j| sys(i as f64 * 10.0, j as f64 * 10.0))).collect();
        grid.push(sys(1e6, 1e6));
        assert!((typical_spacing(&grid) - 10.0).abs() < 1e-9);
        assert_eq!(typical_spacing(&[]), 0.0);
        assert_eq!(typical_spacing(&[sys(1.0, 1.0)]), 0.0);
    }

    fn sys(x: f64, z: f64) -> MapSystem {
        MapSystem {
            id: 0,
            name: String::new(),
            security: 0.0,
            region_id: 0,
            x,
            y: 0.0,
            z,
            x2d: x,
            z2d: z,
        }
    }

    #[test]
    fn projects_center_and_orientation() {
        let systems = [sys(-10.0, -10.0), sys(10.0, 10.0)];
        let b = Bounds::of(&systems).unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, emath::vec2(200.0, 200.0));
        let mid = project(0.0, 0.0, &b, rect, 1.0, Vec2::ZERO);
        assert!((mid.x - 100.0).abs() < 0.5 && (mid.y - 100.0).abs() < 0.5);
        let north = project(0.0, 10.0, &b, rect, 1.0, Vec2::ZERO);
        assert!(north.y < mid.y);
    }
}
