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

/// A flattened 2D layout in the spirit of EVE's in-game star map. EVE ships a
/// precomputed `position2D` per system; lacking that here, we approximate it: each
/// **region** stays anchored at its true geographic centroid (so the big
/// inter-region distances and the New Eden shape are preserved), and only the
/// systems *within* a region are relaxed to gain a minimum spacing — that local
/// crowding is the real artefact of collapsing 3D coordinates onto the x/z plane.
/// Returns clones of `systems` with `x`/`z` replaced (same order).
pub fn spaced_layout(systems: &[MapSystem], graph: &crate::geo::Systems) -> Vec<MapSystem> {
    if systems.is_empty() {
        return Vec::new();
    }
    let mut out = systems.to_vec();

    // Group system indices by region.
    let mut by_region: std::collections::HashMap<i64, Vec<usize>> = std::collections::HashMap::new();
    for (i, s) in systems.iter().enumerate() {
        by_region.entry(s.region_id).or_default().push(i);
    }

    for idxs in by_region.values() {
        let m = idxs.len();
        if m < 2 {
            continue;
        }
        // Geographic centroid + extent of the region (the anchor + scale).
        let (mut cx, mut cz) = (0.0f64, 0.0f64);
        for &i in idxs {
            cx += systems[i].x;
            cz += systems[i].z;
        }
        cx /= m as f64;
        cz /= m as f64;
        let mut ext = 1.0f64;
        for &i in idxs {
            ext = ext.max(((systems[i].x - cx).powi(2) + (systems[i].z - cz).powi(2)).sqrt());
        }

        // Local positions in a roughly [-1, 1] frame, seeded geographically.
        let mut pos: Vec<(f64, f64)> =
            idxs.iter().map(|&i| ((systems[i].x - cx) / ext, (systems[i].z - cz) / ext)).collect();
        let local: std::collections::HashMap<i64, usize> =
            idxs.iter().enumerate().map(|(li, &i)| (systems[i].id, li)).collect();
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (li, &i) in idxs.iter().enumerate() {
            for &nb in graph.neighbors(systems[i].id) {
                if let Some(&lj) = local.get(&nb) {
                    if li < lj {
                        edges.push((li, lj));
                    }
                }
            }
        }

        let k = 2.0 / (m as f64).sqrt(); // target spacing within the region frame
        let cell = k;
        let cutoff = 2.0 * k;
        let iters = 90;
        for it in 0..iters {
            let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> =
                std::collections::HashMap::new();
            for (li, p) in pos.iter().enumerate() {
                grid.entry(((p.0 / cell) as i32, (p.1 / cell) as i32)).or_default().push(li);
            }
            let mut disp = vec![(0.0f64, 0.0f64); m];
            // Local repulsion enforces the minimum spacing (de-overlap).
            for li in 0..m {
                let (gx0, gy0) = ((pos[li].0 / cell) as i32, (pos[li].1 / cell) as i32);
                for gx in (gx0 - 1)..=(gx0 + 1) {
                    for gy in (gy0 - 1)..=(gy0 + 1) {
                        let Some(bucket) = grid.get(&(gx, gy)) else {
                            continue;
                        };
                        for &lj in bucket {
                            if lj == li {
                                continue;
                            }
                            let dx = pos[li].0 - pos[lj].0;
                            let dy = pos[li].1 - pos[lj].1;
                            let d = (dx * dx + dy * dy).sqrt().max(1e-5);
                            if d < cutoff {
                                let f = k * k / d;
                                disp[li].0 += dx / d * f;
                                disp[li].1 += dy / d * f;
                            }
                        }
                    }
                }
            }
            // Weak springs along intra-region gates keep neighbours chained.
            for &(a, c) in &edges {
                let dx = pos[a].0 - pos[c].0;
                let dy = pos[a].1 - pos[c].1;
                let d = (dx * dx + dy * dy).sqrt().max(1e-5);
                let f = d * d / k * 0.5;
                disp[a].0 -= dx / d * f;
                disp[a].1 -= dy / d * f;
                disp[c].0 += dx / d * f;
                disp[c].1 += dy / d * f;
            }
            let temp = 0.1 * (1.0 - it as f64 / iters as f64);
            for li in 0..m {
                let dl = (disp[li].0 * disp[li].0 + disp[li].1 * disp[li].1).sqrt().max(1e-5);
                let lim = dl.min(temp);
                pos[li].0 += disp[li].0 / dl * lim;
                pos[li].1 += disp[li].1 / dl * lim;
            }
        }

        // Map the local frame back to geographic coords (anchor + region scale).
        for (li, &i) in idxs.iter().enumerate() {
            out[i].x = cx + pos[li].0 * ext;
            out[i].z = cz + pos[li].1 * ext;
        }
    }

    out
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
