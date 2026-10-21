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
pub(crate) fn erf_inv(x: f64) -> f64 {
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

/// A flock's spread across its line and up and down, against its spread along it.
pub const FLOCK_ACROSS: f64 = 0.4;
pub const FLOCK_UP: f64 = 0.25;

/// A standard normal draw for ship `i`, axis `k`: seeded, so the same fleet always sits the same
/// way, and without a pattern the eye picks up.
fn normal(i: usize, k: u64) -> f64 {
    let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ k.wrapping_mul(0xD1B5_4A32_D192_ED03);
    let mut next = || {
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    };
    let (u, v) = (next().max(1e-12), next());
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

/// Where `n` ships sit around their anchor (at the origin), a flock strung out along `along`
/// (any length; the x axis when zero): on a sphere for a ball, directions spread evenly; a flock
/// as a swarm, each ship's offset drawn from a bell curve of spread σ along its line,
/// `FLOCK_ACROSS`·σ across it and `FLOCK_UP`·σ up and down, so it is densest in the middle and
/// thins out to the edges. The anchor is the first ship of a flock.
pub fn positions_along(formation: Formation, n: usize, along: Vec3) -> Vec<Vec3> {
    match formation {
        Formation::Ball { radius_m } => directions(n.max(1)).into_iter().take(n).map(|d| d.map(|x| x * f64::from(radius_m))).collect(),
        Formation::Flock { sigma_m } => {
            let l = len(along);
            let ax = if l > 1e-9 { along.map(|x| x / l) } else { [1.0, 0.0, 0.0] };
            // Across on the level where it can be, then up completes the frame.
            let mut side = [-ax[2], 0.0, ax[0]];
            if len(side) < 1e-6 {
                side = [0.0, 0.0, 1.0];
            }
            let side = side.map(|x| x / len(side));
            let up = [ax[1] * side[2] - ax[2] * side[1], ax[2] * side[0] - ax[0] * side[2], ax[0] * side[1] - ax[1] * side[0]];
            let sigma = f64::from(sigma_m);
            (0..n)
                .map(|i| {
                    if i == 0 {
                        return [0.0; 3];
                    }
                    let (a, b, c) = (normal(i, 1) * sigma, normal(i, 2) * sigma * FLOCK_ACROSS, normal(i, 3) * sigma * FLOCK_UP);
                    [ax[0] * a + side[0] * b + up[0] * c, ax[1] * a + side[1] * b + up[1] * c, ax[2] * a + side[2] * b + up[2] * c]
                })
                .collect()
        }
    }
}

/// `positions_along` with the flock strung out along the x axis.
pub fn positions(formation: Formation, n: usize) -> Vec<Vec3> {
    positions_along(formation, n, [1.0, 0.0, 0.0])
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
    // A flock strings out along its heading, moving or not.
    let pa = positions_along(a, a_n, velocity(1.0, a_heading, [1.0, 0.0, 0.0]));
    let pb: Vec<Vec3> = positions_along(b, b_n, velocity(1.0, b_heading, [-1.0, 0.0, 0.0])).into_iter().map(|p| [p[0] + range, p[1], p[2]]).collect();
    let va = velocity(a_speed, a_heading, [1.0, 0.0, 0.0]);
    let vb = velocity(b_speed, b_heading, [-1.0, 0.0, 0.0]);
    pa.iter().map(|s| pb.iter().map(|t| pair(*s, va, *t, vb)).collect()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ball_sits_on_its_radius_and_a_flock_is_a_swarm_strung_out_along_its_heading() {
        let ball = positions(Formation::Ball { radius_m: 5000.0 }, 50);
        assert!(ball.iter().all(|p| (len(*p) - 5000.0).abs() < 1e-6));
        let n = 2000;
        let flock = positions_along(Formation::Flock { sigma_m: 4000.0 }, n, [0.0, 0.0, 3.0]);
        assert_eq!(flock[0], [0.0; 3], "the anchor sits at the centre");
        let sd = |k: usize| (flock.iter().map(|p| p[k] * p[k]).sum::<f64>() / n as f64).sqrt();
        assert!((sd(2) / 4000.0 - 1.0).abs() < 0.08, "σ along the heading (z here): {}", sd(2));
        assert!((sd(0) / (4000.0 * FLOCK_ACROSS) - 1.0).abs() < 0.08, "narrower across: {}", sd(0));
        assert!((sd(1) / (4000.0 * FLOCK_UP) - 1.0).abs() < 0.08, "flatter up and down: {}", sd(1));
        // Denser in the middle: a bell puts about 68% within one σ along the line.
        let near = flock.iter().filter(|p| p[2].abs() < 4000.0).count() as f64 / n as f64;
        assert!((near - 0.683).abs() < 0.04, "{near}");
        assert_eq!(flock, positions_along(Formation::Flock { sigma_m: 4000.0 }, n, [0.0, 0.0, 3.0]), "the same every time");
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
