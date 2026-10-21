//! The 3D scene of a fight: where the two fleets sit and which way they head, and bubbles that set
//! the scene without touching the numbers. Placing the fleets sets the range and both headings.

use serde::{Deserialize, Serialize};

pub type V3 = [f32; 3];

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn scale(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        [1.0, 0.0, 0.0]
    }
}

/// A warp disruption bubble, by what makes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bubble {
    AnchoredSmallI,
    AnchoredSmallII,
    AnchoredMediumI,
    AnchoredMediumII,
    AnchoredLargeI,
    AnchoredLargeII,
    InterdictorProbe,
    SurgicalProbe,
    HeavyInterdictor,
}

impl Bubble {
    pub const ALL: [Bubble; 9] = [Bubble::InterdictorProbe, Bubble::SurgicalProbe, Bubble::HeavyInterdictor, Bubble::AnchoredSmallI, Bubble::AnchoredSmallII, Bubble::AnchoredMediumI, Bubble::AnchoredMediumII, Bubble::AnchoredLargeI, Bubble::AnchoredLargeII];

    /// Its radius in metres: the mobile warp disruptors' and the probes' disruption range, a heavy
    /// interdictor's field generator unscripted.
    pub fn radius(self) -> f32 {
        match self {
            Bubble::AnchoredSmallI => 5_000.0,
            Bubble::AnchoredSmallII => 7_500.0,
            Bubble::AnchoredMediumI => 11_500.0,
            Bubble::AnchoredMediumII => 17_500.0,
            Bubble::AnchoredLargeI => 26_500.0,
            Bubble::AnchoredLargeII => 40_000.0,
            Bubble::InterdictorProbe => 20_000.0,
            Bubble::SurgicalProbe => 10_000.0,
            Bubble::HeavyInterdictor => 20_000.0,
        }
    }

    pub fn anchored(self) -> bool {
        !matches!(self, Bubble::InterdictorProbe | Bubble::SurgicalProbe | Bubble::HeavyInterdictor)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneObject {
    pub bubble: Bubble,
    pub at: V3,
}

/// Positions in metres, y up. Directions are where each fleet heads; their speed shares are the
/// fleets' own motion knobs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub own_at: V3,
    pub enemy_at: V3,
    pub own_dir: V3,
    pub enemy_dir: V3,
    /// The opponent's speed share, set here since its fleet is another document.
    pub enemy_share: f32,
    pub objects: Vec<SceneObject>,
}

impl Default for Scene {
    fn default() -> Self {
        Scene { own_at: [0.0; 3], enemy_at: [30_000.0, 0.0, 0.0], own_dir: [0.0, 0.0, 1.0], enemy_dir: [0.0, 0.0, 1.0], enemy_share: 1.0, objects: Vec::new() }
    }
}

/// The angle in degrees between heading `dir` and the line to `other` from `at`: 0 straight at
/// it, 90 across, 180 away.
pub fn heading(at: V3, dir: V3, other: V3) -> f32 {
    let line = norm(sub(other, at));
    dot(norm(dir), line).clamp(-1.0, 1.0).acos().to_degrees()
}

/// `dir` turned to make `deg` with the line, keeping the side it leans to (straight across on
/// the level when it lies on the line).
fn turned(at: V3, dir: V3, other: V3, deg: f32) -> V3 {
    let line = norm(sub(other, at));
    let mut side = sub(norm(dir), scale(line, dot(norm(dir), line)));
    if len(side) < 1e-4 {
        side = cross([0.0, 1.0, 0.0], line);
        if len(side) < 1e-4 {
            side = [0.0, 0.0, 1.0];
        }
    }
    let side = norm(side);
    let r = deg.to_radians();
    add(scale(line, r.cos()), scale(side, r.sin()))
}

impl Scene {
    pub fn range(&self) -> f32 {
        len(sub(self.enemy_at, self.own_at))
    }

    /// The scene as the fleets have it: the opponent moved along the line to their range, each
    /// heading turned to the knobs' angle. Called before drawing, so a slider moved elsewhere
    /// shows here.
    pub fn follow(&mut self, range: f32, own_heading: f32, enemy: Option<(f32, f32)>) {
        let line = norm(sub(self.enemy_at, self.own_at));
        if (self.range() - range).abs() > 1.0 {
            self.enemy_at = add(self.own_at, scale(line, range.max(1.0)));
        }
        if (heading(self.own_at, self.own_dir, self.enemy_at) - own_heading).abs() > 0.5 {
            self.own_dir = turned(self.own_at, self.own_dir, self.enemy_at, own_heading);
        }
        if let Some((share, h)) = enemy {
            self.enemy_share = share;
            if (heading(self.enemy_at, self.enemy_dir, self.own_at) - h).abs() > 0.5 {
                self.enemy_dir = turned(self.enemy_at, self.enemy_dir, self.own_at, h);
            }
        }
    }

    /// What placing the fleets says: the range, the own heading, and the opponent's motion.
    pub fn read(&self) -> (f32, f32, (f32, f32)) {
        (self.range(), heading(self.own_at, self.own_dir, self.enemy_at), (self.enemy_share, heading(self.enemy_at, self.enemy_dir, self.own_at)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placing_the_fleets_sets_range_and_headings_and_the_knobs_move_the_scene() {
        let mut s = Scene { own_at: [0.0; 3], enemy_at: [0.0, 0.0, 40_000.0], own_dir: [0.0, 0.0, 1.0], enemy_dir: [1.0, 0.0, 0.0], ..Scene::default() };
        let (range, own, (share, enemy)) = s.read();
        assert!((range - 40_000.0).abs() < 1e-3);
        assert!(own.abs() < 1e-3, "heading straight at them");
        assert!((enemy - 90.0).abs() < 1e-3, "they cross");
        assert_eq!(share, 1.0);
        s.follow(10_000.0, 180.0, Some((0.5, 0.0)));
        let (range, own, (share, enemy)) = s.read();
        assert!((range - 10_000.0).abs() < 1.0, "the range moves the opponent along the line");
        assert!((own - 180.0).abs() < 0.5, "turned away: {own}");
        assert!(enemy.abs() < 0.5 && share == 0.5);
        s.follow(10_000.0, 90.0, None);
        assert!((s.read().1 - 90.0).abs() < 0.5);
    }

    #[test]
    fn bubbles_have_their_real_radii() {
        assert_eq!(Bubble::AnchoredLargeII.radius(), 40_000.0);
        assert_eq!(Bubble::InterdictorProbe.radius(), 20_000.0);
        assert!(Bubble::ALL.iter().all(|b| b.radius() > 0.0));
        assert!(Bubble::AnchoredSmallI.anchored() && !Bubble::HeavyInterdictor.anchored());
    }
}
