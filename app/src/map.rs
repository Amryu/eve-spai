//! 2D map projection (docs/DESIGN.md §7.1 E5). Projects EVE solar-system
//! coordinates (x/z top-down plane) into a screen rect with margin, uniform scale,
//! zoom, and pan. EVE +z is "north", so it is flipped to point up.

use egui::{Pos2, Rect, Vec2};

use crate::store::MapSystem;

/// What the map is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapView {
    Universe,
    Region(i64),
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

    /// Uniform scale to fit the region in `rect` (before zoom).
    fn base_scale(&self, rect: Rect, margin: f32) -> f32 {
        let w = (rect.width() - 2.0 * margin).max(1.0) as f64;
        let h = (rect.height() - 2.0 * margin).max(1.0) as f64;
        let span_x = (self.max_x - self.min_x).max(1.0);
        let span_z = (self.max_z - self.min_z).max(1.0);
        (w / span_x).min(h / span_z) as f32
    }
}

/// Metres per light-year (EVE map distances).
pub const LY_METERS: f64 = 9.460_730_472_580_8e15;

/// Max jump-drive ranges (light-years) at maxed skills (Jump Drive Calibration V).
/// Capitals share a 5 ly base; Black Ops reach further; Jump Freighters furthest.
pub const JUMP_RANGES: &[(&str, f64)] = &[
    ("Capital", 5.0),
    ("Black Ops", 8.0),
    ("Jump Freighter", 10.0),
];

/// True 3D distance between two systems, in light-years.
pub fn ly_distance(a: &MapSystem, b: &MapSystem) -> f64 {
    let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
    d / LY_METERS
}

/// Screen length (pixels) of `ly` light-years at the current projection scale.
pub fn ly_to_pixels(ly: f64, b: &Bounds, rect: Rect, zoom: f32) -> f32 {
    (ly * LY_METERS) as f32 * b.base_scale(rect, 30.0) * zoom
}

/// A flattened 2D layout like EVE's in-game star map: seeded from the true
/// geographic x/z (so it keeps the New Eden shape) then relaxed so neighbouring
/// systems gain a minimum spacing instead of overlapping (which they otherwise do
/// when 3D coordinates collapse onto the x/z plane). Local repulsion via a uniform
/// grid keeps it O(n) so it scales to the whole cluster. Returns clones of
/// `systems` with `x`/`z` replaced by the layout coordinates (same order).
pub fn spaced_layout(systems: &[MapSystem], graph: &crate::geo::Systems) -> Vec<MapSystem> {
    let n = systems.len();
    if n == 0 {
        return Vec::new();
    }
    let idx: std::collections::HashMap<i64, usize> =
        systems.iter().enumerate().map(|(i, s)| (s.id, i)).collect();

    // Seed from normalised geographic coords so the result stays recognisable.
    let Some(b) = Bounds::of(systems) else {
        return systems.to_vec();
    };
    let sx = (b.max_x - b.min_x).max(1.0);
    let sz = (b.max_z - b.min_z).max(1.0);
    let mut pos: Vec<(f64, f64)> = systems
        .iter()
        .map(|s| ((s.x - b.min_x) / sx, (s.z - b.min_z) / sz))
        .collect();

    let mut edges: Vec<(usize, usize)> = Vec::new();
    for s in systems {
        let a = idx[&s.id];
        for &nb in graph.neighbors(s.id) {
            if let Some(&b) = idx.get(&nb) {
                if a < b {
                    edges.push((a, b));
                }
            }
        }
    }

    let k = (1.0 / n as f64).sqrt(); // target spacing in the unit square
    let cell = k; // grid cell ≈ interaction radius
    let cutoff = 2.0 * k;
    let iters = 80;
    for it in 0..iters {
        // Bucket nodes into a uniform grid for local-neighbour queries.
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> =
            std::collections::HashMap::new();
        for (i, p) in pos.iter().enumerate() {
            grid.entry(((p.0 / cell) as i32, (p.1 / cell) as i32)).or_default().push(i);
        }
        let mut disp = vec![(0.0f64, 0.0f64); n];
        // Local repulsion: only against nodes in the 3x3 cell neighbourhood.
        for i in 0..n {
            let (cx, cy) = ((pos[i].0 / cell) as i32, (pos[i].1 / cell) as i32);
            for gx in (cx - 1)..=(cx + 1) {
                for gy in (cy - 1)..=(cy + 1) {
                    let Some(bucket) = grid.get(&(gx, gy)) else {
                        continue;
                    };
                    for &j in bucket {
                        if j == i {
                            continue;
                        }
                        let dx = pos[i].0 - pos[j].0;
                        let dy = pos[i].1 - pos[j].1;
                        let d = (dx * dx + dy * dy).sqrt().max(1e-5);
                        if d < cutoff {
                            let f = k * k / d;
                            disp[i].0 += dx / d * f;
                            disp[i].1 += dy / d * f;
                        }
                    }
                }
            }
        }
        // Weak attraction along gate edges keeps neighbours chained.
        for &(a, c) in &edges {
            let dx = pos[a].0 - pos[c].0;
            let dy = pos[a].1 - pos[c].1;
            let d = (dx * dx + dy * dy).sqrt().max(1e-5);
            let f = d * d / k;
            disp[a].0 -= dx / d * f;
            disp[a].1 -= dy / d * f;
            disp[c].0 += dx / d * f;
            disp[c].1 += dy / d * f;
        }
        // Cap displacement, cooling over time.
        let temp = 0.05 * (1.0 - it as f64 / iters as f64);
        for i in 0..n {
            let dl = (disp[i].0 * disp[i].0 + disp[i].1 * disp[i].1).sqrt().max(1e-5);
            let lim = dl.min(temp);
            pos[i].0 += disp[i].0 / dl * lim;
            pos[i].1 += disp[i].1 / dl * lim;
        }
    }

    systems
        .iter()
        .enumerate()
        .map(|(i, s)| MapSystem {
            x: pos[i].0 * 1000.0,
            z: pos[i].1 * 1000.0,
            ..s.clone()
        })
        .collect()
}

pub fn project(x: f64, z: f64, b: &Bounds, rect: Rect, zoom: f32, pan: Vec2) -> Pos2 {
    let scale = b.base_scale(rect, 30.0) * zoom;
    let center = rect.center() + pan;
    Pos2::new(
        center.x + ((x - b.mid_x()) as f32) * scale,
        center.y - ((z - b.mid_z()) as f32) * scale, // flip z -> north up
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys(x: f64, z: f64) -> MapSystem {
        MapSystem {
            id: 0,
            name: String::new(),
            security: 0.0,
            region_id: 0,
            x,
            y: 0.0,
            z,
        }
    }

    #[test]
    fn projects_center_and_orientation() {
        let systems = [sys(-10.0, -10.0), sys(10.0, 10.0)];
        let b = Bounds::of(&systems).unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(200.0, 200.0));
        // Midpoint projects to the rect centre (no pan/zoom).
        let mid = project(0.0, 0.0, &b, rect, 1.0, Vec2::ZERO);
        assert!((mid.x - 100.0).abs() < 0.5 && (mid.y - 100.0).abs() < 0.5);
        // Higher z is further up (smaller screen y).
        let north = project(0.0, 10.0, &b, rect, 1.0, Vec2::ZERO);
        assert!(north.y < mid.y);
    }
}
