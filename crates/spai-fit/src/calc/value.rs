//! One attribute value from its base and its modifiers: the operator order, how each operator
//! aggregates, and the stacking penalty.

use crate::dogma::Op;
use crate::ids::AttrId;

use super::ItemIdx;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Attr(ItemIdx, AttrId),
    Const(f64),
    /// A propulsion module's speed bonus in percent: its `speedFactor × speedBoostFactor` over
    /// the ship's mass, which only exists once the mass is known.
    PropBonus(ItemIdx),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Modification {
    pub op: Op,
    pub source: Source,
    pub penalised: bool,
}

/// The k-th strongest penalised modifier (k from 0) keeps `PENALTY_FACTOR^(k²)` of its effect:
/// `exp(-(k / 2.67)²)`, so 100%, 86.9%, 57.1%, 28.3%, 10.5%, 3.0%, ...
pub const PENALTY_FACTOR: f64 = 0.869_119_980_800_397_4;

/// Where an operator sits in the order of application.
pub(crate) fn order_of(op: Op) -> usize {
    ORDER.iter().position(|o| *o == op).unwrap_or(ORDER.len())
}

const ORDER: [Op; 9] = [Op::PreAssign, Op::PreMul, Op::PreDiv, Op::ModAdd, Op::ModSub, Op::PostMul, Op::PostDiv, Op::PostPercent, Op::PostAssign];

/// `mods` are `(operator, modifying value, penalised)`.
pub fn combine(base: f64, high_is_good: bool, mods: &[(Op, f64, bool)]) -> f64 {
    let mut v = base;
    for op in ORDER {
        let group: Vec<(f64, bool)> = mods.iter().filter(|(o, _, _)| *o == op).map(|(_, x, p)| (*x, *p)).collect();
        if group.is_empty() {
            continue;
        }
        match op {
            Op::PreAssign | Op::PostAssign => {
                let it = group.iter().map(|(x, _)| *x);
                v = if high_is_good { it.fold(f64::NEG_INFINITY, f64::max) } else { it.fold(f64::INFINITY, f64::min) };
            }
            Op::ModAdd => v += group.iter().map(|(x, _)| *x).sum::<f64>(),
            Op::ModSub => v -= group.iter().map(|(x, _)| *x).sum::<f64>(),
            Op::PreMul | Op::PreDiv | Op::PostMul | Op::PostDiv | Op::PostPercent => {
                let normalise = |x: f64| fraction(op, x);
                let xs: Vec<(f64, bool)> = group.iter().map(|(x, p)| (normalise(*x), *p)).collect();
                let weights = weights_of(&xs);
                v *= xs.iter().zip(weights).map(|((x, _), w)| 1.0 + x * w).product::<f64>();
            }
        }
    }
    v
}

/// The modifying value of a multiplying operator as the fraction it adds: `×1.2` and `+20%` are
/// both `0.2`, `÷1.25` is `-0.2`.
pub fn fraction(op: Op, x: f64) -> f64 {
    match op {
        Op::PreDiv | Op::PostDiv => {
            if x == 0.0 {
                0.0
            } else {
                1.0 / x - 1.0
            }
        }
        Op::PostPercent => x / 100.0,
        Op::PreMul | Op::PostMul => x - 1.0,
        _ => x,
    }
}

/// How much of each modifier of one operator stays after the stacking penalty: all of an
/// unpenalised one, `PENALTY_FACTOR^(k²)` of the k-th strongest penalised one, bonuses and maluses
/// ranked apart. `xs` are `(fraction, penalised)`.
fn weights_of(xs: &[(f64, bool)]) -> Vec<f64> {
    let rank = |same_sign: &dyn Fn(f64) -> bool| {
        let mut idx: Vec<usize> = (0..xs.len()).filter(|&i| xs[i].1 && same_sign(xs[i].0)).collect();
        idx.sort_by(|a, b| xs[*b].0.abs().partial_cmp(&xs[*a].0.abs()).unwrap_or(std::cmp::Ordering::Equal));
        idx
    };
    let mut w = vec![1.0; xs.len()];
    for ranked in [rank(&|x| x > 0.0), rank(&|x| x < 0.0)] {
        for (k, i) in ranked.into_iter().enumerate() {
            w[i] = PENALTY_FACTOR.powi((k * k) as i32);
        }
    }
    w
}

/// The share of each modifier's effect that stays after the stacking penalty, in the order given;
/// 1 for one that is not penalised or does nothing.
pub fn penalty_weights(mods: &[(Op, f64, bool)]) -> Vec<f64> {
    let mut w = vec![1.0; mods.len()];
    for op in ORDER {
        let idx: Vec<usize> = (0..mods.len()).filter(|&i| mods[i].0 == op).collect();
        let xs: Vec<(f64, bool)> = idx.iter().map(|&i| (fraction(op, mods[i].1), mods[i].2)).collect();
        for (i, wi) in idx.into_iter().zip(weights_of(&xs)) {
            w[i] = wi;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn the_penalty_factor_is_the_published_curve() {
        assert!(close(PENALTY_FACTOR, (-(1.0f64 / 2.67).powi(2)).exp()));
        let pen: Vec<f64> = (0..6).map(|k| PENALTY_FACTOR.powi(k * k)).collect();
        for (got, want) in pen.iter().zip([1.0, 0.8691, 0.5706, 0.2830, 0.1060, 0.0300]) {
            assert!((got - want).abs() < 1e-3, "{got} vs {want}");
        }
    }

    #[test]
    fn identical_penalised_bonuses_fall_off_in_the_known_steps() {
        // Four 20% bonuses, as four hardeners of one kind.
        let m: Vec<_> = (0..4).map(|_| (Op::PostPercent, 20.0, true)).collect();
        let want = 1.2 * (1.0 + 0.2 * PENALTY_FACTOR) * (1.0 + 0.2 * PENALTY_FACTOR.powi(4)) * (1.0 + 0.2 * PENALTY_FACTOR.powi(9));
        assert!(close(combine(1.0, true, &m), want), "{}", combine(1.0, true, &m));
        assert!((want - 1.6582).abs() < 1e-3, "four 20% hardeners come to +65.8%, the figure every fitting guide quotes");
        let one = combine(100.0, true, &m[..1]);
        assert!(close(one, 120.0));
    }

    #[test]
    fn unpenalised_modifiers_stack_in_full_and_bonuses_and_maluses_are_sorted_apart() {
        let m = [(Op::PostPercent, 20.0, false), (Op::PostPercent, 20.0, false), (Op::PostPercent, 20.0, true), (Op::PostPercent, -50.0, true), (Op::PostPercent, -10.0, true)];
        let want = 1.2 * 1.2 * 1.2 * 0.5 * (1.0 - 0.1 * PENALTY_FACTOR);
        assert!(close(combine(1.0, true, &m), want));
    }

    #[test]
    fn the_strongest_penalised_modifier_comes_first_whatever_its_order() {
        let a = [(Op::PostMul, 1.1, true), (Op::PostMul, 1.5, true)];
        let b = [(Op::PostMul, 1.5, true), (Op::PostMul, 1.1, true)];
        assert!(close(combine(1.0, true, &a), combine(1.0, true, &b)));
        assert!(close(combine(1.0, true, &a), 1.5 * (1.0 + 0.1 * PENALTY_FACTOR)));
    }

    #[test]
    fn operators_apply_in_order_and_aggregate_as_eve_does() {
        let m = [
            (Op::PostPercent, 50.0, false),
            (Op::ModAdd, 10.0, false),
            (Op::PreMul, 2.0, false),
            (Op::PostMul, 3.0, false),
            (Op::ModSub, 4.0, false),
            (Op::PreDiv, 4.0, false),
            (Op::PostDiv, 2.0, false),
        ];
        // ((10 × 2 / 4) + 10 − 4) × 3 / 2 × 1.5
        assert!(close(combine(10.0, true, &m), 24.75));
        assert!(close(combine(10.0, true, &[(Op::PreAssign, 3.0, false), (Op::PreAssign, 7.0, false), (Op::ModAdd, 1.0, false)]), 8.0));
        assert!(close(combine(10.0, false, &[(Op::PostAssign, 3.0, false), (Op::PostAssign, 7.0, false), (Op::ModAdd, 100.0, false)]), 3.0));
    }

    #[test]
    fn separate_operators_are_separate_penalty_groups() {
        // A Damage Control's PreMul sits in its own group, so it never loses to the hardeners.
        let m = [(Op::PreMul, 0.85, true), (Op::PostPercent, -30.0, true), (Op::PostPercent, -30.0, true)];
        let want = 0.85 * 0.7 * (1.0 - 0.3 * PENALTY_FACTOR);
        assert!(close(combine(1.0, false, &m), want));
    }

    #[test]
    fn a_division_by_zero_modifier_changes_nothing() {
        assert!(close(combine(5.0, true, &[(Op::PostDiv, 0.0, false)]), 5.0));
    }
}
