//! 2D map projection (docs/DESIGN.md §7.1 E5). Projects EVE solar-system
//! coordinates (x/z top-down plane) into a screen rect with margin, uniform scale,
//! zoom, and pan. EVE +z is "north", so it is flipped to point up.

use egui::{Pos2, Rect, Vec2};

use crate::store::MapSystem;

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
            x,
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
