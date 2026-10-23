//! How much of a weapon's damage lands: turret hit chance and wrecking shots, missile damage
//! reduction, drones keeping up. Formulas from EVE University's turret and missile mechanics
//! pages.

use crate::fit::TargetProfile;

use super::{WeaponGroup, WeaponKind};

/// The chance a turret hits: `0.5^((angular × sigRes / (tracking × sig))²) × 0.5^((max(0, d − optimal) / falloff)²)`,
/// `angular` in radians per second. Beyond `optimal + 3 × falloff` the range term is 0.
pub fn turret_hit_chance(distance: f64, transversal: f64, target_sig: f64, optimal: f64, falloff: f64, tracking: f64, sig_res: f64) -> f64 {
    // Closer than a metre counts as one: point blank, anything crossing is too fast to track.
    let angular = transversal.abs() / distance.max(1.0);
    let tracking_term = if tracking > 0.0 && target_sig > 0.0 { 0.5f64.powf((angular * sig_res / (tracking * target_sig)).powi(2)) } else if angular > 0.0 { 0.0 } else { 1.0 };
    let over = (distance - optimal).max(0.0);
    let range_term = if over <= 0.0 {
        1.0
    } else if falloff <= 0.0 || over > 3.0 * falloff {
        0.0
    } else {
        0.5f64.powf((over / falloff).powi(2))
    };
    tracking_term * range_term
}

/// Average damage per shot as a share of the listed damage, from the hit chance: normal hits do
/// `rand + 0.49` of it and one in a hundred is a wrecking shot at three times, so
/// `0.5 × min(cth² + 0.98 cth + 0.0501, 6 cth)`.
pub fn turret_damage_factor(hit_chance: f64) -> f64 {
    let c = hit_chance.clamp(0.0, 1.0);
    0.5 * (c * c + 0.98 * c + 0.0501).min(6.0 * c)
}

/// `min(1, sig / E, ((sig × Ve) / (E × Vt))^drf)`: a big slow target takes it all.
pub fn missile_damage_factor(target_sig: f64, target_velocity: f64, explosion_radius: f64, explosion_velocity: f64, drf: f64) -> f64 {
    if explosion_radius <= 0.0 || target_sig <= 0.0 {
        return 1.0;
    }
    let sig_term = target_sig / explosion_radius;
    let vel_term = if target_velocity <= 0.0 || explosion_velocity <= 0.0 {
        1.0
    } else {
        ((target_sig * explosion_velocity) / (explosion_radius * target_velocity)).powf(drf.max(0.0))
    };
    1.0f64.min(sig_term).min(vel_term)
}

/// What a weapon group lands on the target at `distance`, as a share of its listed damage.
///
/// Drones that fly faster than the target keep up and hit as their listed damage; slower ones
/// are treated as turrets whose transversal is the speed they lack, around their own orbit.
/// Missiles reach their whole seconds of flight surely and one second more by the flight time's
/// fraction of a second.
pub fn applied_factor(w: &WeaponGroup, distance: f64, target: &TargetProfile, drone_speed: f64, drone_control_range: f64) -> f64 {
    let sig = f64::from(target.signature);
    match w.kind {
        WeaponKind::Turret => turret_damage_factor(turret_hit_chance(distance, f64::from(target.transversal), sig, w.optimal, w.falloff, w.tracking, w.signature_resolution)),
        WeaponKind::Missile => {
            let Some(m) = &w.missile else { return 0.0 };
            m.reach(distance) * missile_damage_factor(sig, f64::from(target.velocity), m.explosion_radius, m.explosion_velocity, m.damage_reduction_factor)
        }
        WeaponKind::Drone => {
            if distance > drone_control_range {
                return 0.0;
            }
            let lacking = (f64::from(target.velocity) - drone_speed).max(0.0);
            // A drone that keeps up hits every shot, wrecking shots included, as a turret with
            // no transversal does: a flat 1.0 here dipped below the first step of the curve.
            if lacking <= 0.0 {
                return turret_damage_factor(1.0);
            }
            let orbit = w.optimal.max(500.0);
            turret_damage_factor(turret_hit_chance(orbit, lacking, sig, w.optimal, w.falloff, w.tracking, w.signature_resolution))
        }
        WeaponKind::Vorton => {
            let Some(m) = &w.missile else { return 0.0 };
            if distance > w.optimal {
                return 0.0;
            }
            missile_damage_factor(sig, f64::from(target.velocity), m.explosion_radius, m.explosion_velocity, m.damage_reduction_factor)
        }
        WeaponKind::Bomb => {
            let Some(m) = &w.missile else { return 0.0 };
            // It bursts where its flight ends; what sits within its blast takes the share its
            // signature is of the explosion.
            if (distance - m.range).abs() > w.splash {
                return 0.0;
            }
            if m.explosion_radius > 0.0 { (sig / m.explosion_radius).min(1.0) } else { 1.0 }
        }
        WeaponKind::Breacher => w.missile.as_ref().map_or(0.0, |m| m.reach(distance)),
        WeaponKind::Superweapon => {
            if distance > w.optimal {
                return 0.0;
            }
            if w.signature_resolution > 0.0 { (sig / w.signature_resolution).min(1.0) } else { 1.0 }
        }
        WeaponKind::Smartbomb => {
            if distance <= w.optimal {
                1.0
            } else {
                0.0
            }
        }
        WeaponKind::Fighter => {
            let Some(m) = &w.missile else { return 1.0 };
            crate::stats::range_factor(distance, w.optimal, w.falloff) * missile_damage_factor(sig, f64::from(target.velocity), m.explosion_radius, m.explosion_velocity, m.damage_reduction_factor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_still_target_in_optimal_is_always_hit_and_falloff_halves_at_one_falloff() {
        assert!(close(turret_hit_chance(1000.0, 0.0, 100.0, 1200.0, 6000.0, 0.5, 40_000.0), 1.0));
        assert!(close(turret_hit_chance(7200.0, 0.0, 100.0, 1200.0, 6000.0, 0.5, 40_000.0), 0.5));
        assert!(close(turret_hit_chance(0.0, 0.0, 100.0, 1200.0, 6000.0, 0.5, 40_000.0), 1.0), "point blank and still");
        assert!(turret_hit_chance(0.0, 300.0, 100.0, 1200.0, 6000.0, 0.5, 40_000.0) < 1e-6, "point blank and crossing: too fast to track");
        assert!(turret_hit_chance(0.0, 300.0, 100.0, 1200.0, 6000.0, 0.5, 40_000.0) <= turret_hit_chance(100.0, 300.0, 100.0, 1200.0, 6000.0, 0.5, 40_000.0));
        assert!(close(turret_hit_chance(1200.0 + 18_000.0 + 1.0, 0.0, 100.0, 1200.0, 6000.0, 0.5, 40_000.0), 0.0), "turrets are cut off past three falloffs");
        assert!(close(turret_hit_chance(2000.0, 0.0, 100.0, 1200.0, 0.0, 0.5, 40_000.0), 0.0), "no falloff: nothing past optimal");
    }

    #[test]
    fn tracking_follows_the_published_curve() {
        // 425mm AutoCannon II: tracking 33.792 at a 40 km signature resolution, a 400 m target at
        // 0.1 rad/s: (0.1 × 40000 / (33.792 × 400))² = 0.0876.
        let c = turret_hit_chance(10_000.0, 1000.0, 400.0, 20_000.0, 10_000.0, 33.792, 40_000.0);
        assert!((c - 0.5f64.powf(0.0876)).abs() < 1e-3, "{c}");
        assert!(turret_hit_chance(10_000.0, 1000.0, 40.0, 20_000.0, 10_000.0, 33.792, 40_000.0) < 0.1, "a frigate at the same speed is barely hit");
    }

    #[test]
    fn the_damage_factor_counts_wrecking_shots() {
        assert!(close(turret_damage_factor(1.0), 0.5 * (1.0 + 0.98 + 0.0501)));
        assert!(close(turret_damage_factor(0.005), 0.5 * 6.0 * 0.005), "below a 1% chance every hit is a wrecking one");
        assert!(close(turret_damage_factor(0.0), 0.0));
        assert!(turret_damage_factor(0.5) > 0.3 && turret_damage_factor(0.5) < 0.4);
    }

    #[test]
    fn missiles_land_in_full_on_big_slow_targets_and_less_on_small_fast_ones() {
        assert!(close(missile_damage_factor(400.0, 0.0, 140.0, 85.0, 0.682), 1.0));
        assert!(close(missile_damage_factor(70.0, 0.0, 140.0, 85.0, 0.682), 0.5), "signature half the explosion radius: half the damage");
        let fast = missile_damage_factor(140.0, 850.0, 140.0, 85.0, 0.682);
        assert!((fast - 0.1f64.powf(0.682)).abs() < 1e-9, "{fast}");
        assert!(close(missile_damage_factor(100.0, 100.0, 0.0, 85.0, 0.682), 1.0));
    }
}
