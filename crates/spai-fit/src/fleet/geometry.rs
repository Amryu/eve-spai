//! Where a fleet's ships sit and how they move, for distances and angular velocities between
//! every shooter and every target.

use super::Formation;

pub type Vec3 = [f64; 3];

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

/// The golden angle, for spreading directions evenly.
const GOLDEN: f64 = 2.399_963_229_728_653;

/// `n` directions spread evenly over a sphere, the same every time.
fn directions(n: usize) -> Vec<Vec3> {
    (0..n)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let phi = i as f64 * GOLDEN;
            [phi.cos() * r, y, phi.sin() * r]
        })
        .collect()
}

/// The inverse error function, to about 1e-7: what turns an even spread of shares into a
/// bell-curve spread of distances.
fn erf_inv(x: f64) -> f64 {
    let x = x.clamp(-0.999_999, 0.999_999);
    let w = -((1.0 - x) * (1.0 + x)).ln();
    if w < 5.0 {
        let w = w - 2.5;
        let mut p = 2.810_226_36e-08;
        for c in [3.432_739_39e-07, -3.523_387_7e-06, -4.391_506_54e-06, 0.000_218_580_87, -0.001_253_725_03, -0.004_177_681_64, 0.246_640_727, 1.501_409_41] {
            p = c + p * w;
        }
        p * x
    } else {
        let w = w.sqrt() - 3.0;
        let mut p = -0.000_200_214_257;
        for c in [0.000_100_950_558, 0.001_349_343_22, -0.003_673_428_44, 0.005_739_507_73, -0.007_622_461_3, 0.009_438_870_47, 1.001_674_06, 2.832_976_82] {
            p = c + p * w;
        }
        p * x
    }
}

/// Where `n` ships sit around their anchor (at the origin): on a sphere for a ball; for a flock,
/// at distances that follow a bell curve of the given spread (a half-normal), in directions
/// spread evenly. The anchor itself is the first ship of a flock.
pub fn positions(formation: Formation, n: usize) -> Vec<Vec3> {
    let dirs = directions(n.max(1));
    match formation {
        Formation::Ball { radius_m } => dirs.into_iter().take(n).map(|d| d.map(|x| x * f64::from(radius_m))).collect(),
        Formation::Flock { sigma_m } => (0..n)
            .map(|i| {
                // Shares spread evenly but shuffled against the directions, so near and far
                // ships are not bunched on one side.
                let share = ((i as f64 * 0.618_033_988_75) + 0.5).fract() * 0.999;
                let r = if i == 0 { 0.0 } else { f64::from(sigma_m) * std::f64::consts::SQRT_2 * erf_inv(share) };
                dirs[i].map(|x| x * r)
            })
            .collect(),
    }
}

/// A fleet's velocity: `speed` along its heading against the line to the enemy, 0° towards it,
/// 90° across (the same side for both fleets), 180° away. `towards` is the direction of the enemy.
pub fn velocity(speed: f64, heading_deg: f64, towards: Vec3) -> Vec3 {
    let h = heading_deg.to_radians();
    let across = [0.0, 1.0, 0.0];
    [towards[0] * h.cos() * speed + across[0] * h.sin() * speed, towards[1] * h.cos() * speed + across[1] * h.sin() * speed, towards[2] * h.cos() * speed + across[2] * h.sin() * speed]
}

/// Between a shooter and a target: how far apart, how fast the target moves across the line
/// between them, and the angle it sweeps per second from the shooter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    pub distance: f64,
    pub transversal: f64,
    pub angular: f64,
}

pub fn pair(shooter: Vec3, shooter_v: Vec3, target: Vec3, target_v: Vec3) -> Pair {
    let r = sub(target, shooter);
    let distance = len(r).max(1.0);
    let unit = r.map(|x| x / distance);
    let rel = sub(target_v, shooter_v);
    let along = dot(rel, unit);
    let across = sub(rel, unit.map(|x| x * along));
    let transversal = len(across);
    Pair { distance, transversal, angular: transversal / distance }
}

/// Two fleets `range` apart, anchors on the x axis: every shooter of `a` against every target
/// of `b`, with each fleet moving at its own speed and heading.
pub fn engagement(a: Formation, a_n: usize, a_speed: f64, a_heading: f64, b: Formation, b_n: usize, b_speed: f64, b_heading: f64, range: f64) -> Vec<Vec<Pair>> {
    let pa = positions(a, a_n);
    let pb: Vec<Vec3> = positions(b, b_n).into_iter().map(|p| [p[0] + range, p[1], p[2]]).collect();
    let va = velocity(a_speed, a_heading, [1.0, 0.0, 0.0]);
    let vb = velocity(b_speed, b_heading, [-1.0, 0.0, 0.0]);
    pa.iter().map(|s| pb.iter().map(|t| pair(*s, va, *t, vb)).collect()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ball_sits_on_its_radius_and_a_flock_spreads_like_a_bell() {
        let ball = positions(Formation::Ball { radius_m: 5000.0 }, 50);
        assert!(ball.iter().all(|p| (len(*p) - 5000.0).abs() < 1e-6));
        let flock = positions(Formation::Flock { sigma_m: 4000.0 }, 400);
        let dists: Vec<f64> = flock.iter().map(|p| len(*p)).collect();
        let mean = dists.iter().sum::<f64>() / dists.len() as f64;
        // A half-normal of spread σ averages σ·√(2/π).
        assert!((mean / (4000.0 * (2.0 / std::f64::consts::PI).sqrt()) - 1.0).abs() < 0.05, "mean {mean}");
        assert_eq!(dists[0], 0.0, "the anchor sits at the centre");
        let near = dists.iter().filter(|d| **d < 4000.0).count() as f64 / dists.len() as f64;
        assert!((near - 0.683).abs() < 0.05, "about two thirds within one σ: {near}");
    }

    #[test]
    fn crossing_fleets_sweep_angles_and_spread_widens_them() {
        // Fleet B crosses at 1000 m/s, A sits still, 20 km apart.
        let tight = engagement(Formation::Ball { radius_m: 0.0 }, 1, 0.0, 0.0, Formation::Ball { radius_m: 0.0 }, 1, 1000.0, 90.0, 20_000.0);
        let p = tight[0][0];
        assert!((p.transversal - 1000.0).abs() < 1e-6 && (p.angular - 0.05).abs() < 1e-9);
        // Both crossing the same way at the same speed: nothing across between them.
        let along = engagement(Formation::Ball { radius_m: 0.0 }, 1, 1000.0, 90.0, Formation::Ball { radius_m: 0.0 }, 1, 1000.0, 90.0, 20_000.0);
        assert!(along[0][0].transversal < 1e-6);
        // Spread out, the same motion gives a range of angles across the shooters.
        let spread = engagement(Formation::Flock { sigma_m: 8000.0 }, 30, 0.0, 0.0, Formation::Ball { radius_m: 0.0 }, 1, 1000.0, 90.0, 20_000.0);
        let angles: Vec<f64> = spread.iter().map(|row| row[0].angular).collect();
        let (lo, hi) = angles.iter().fold((f64::MAX, 0.0f64), |(a, b), x| (a.min(*x), b.max(*x)));
        assert!(hi > lo * 1.5, "spread shooters see the target at different angles: {lo}..{hi}");
    }
}
