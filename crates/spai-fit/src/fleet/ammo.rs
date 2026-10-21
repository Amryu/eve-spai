//! Which charge does most at each range: a weapon's figures with every charge it can load, the
//! carried ones (loaded or in the cargo) ranked first, a better one not carried as a suggestion.

use crate::calc::Env;
use crate::dogma::Dogma;
use crate::fit::{Fit, TargetProfile};
use crate::ids::TypeId;
use crate::skills::SkillSet;
use crate::stats::application::applied_factor;
use crate::stats::WeaponGroup;

/// One weapon type of a fit with each charge it could fire.
#[derive(Clone, Debug)]
pub struct AmmoChoice {
    pub weapon: TypeId,
    pub loaded: Option<TypeId>,
    /// (charge, carried, the weapon group as it would be with it).
    pub options: Vec<(TypeId, bool, WeaponGroup)>,
}

/// Every charged weapon type of the fit with its options: the fit worked out again with each
/// compatible charge in all of that type's modules.
pub fn choices(dogma: &Dogma, fit: &Fit, skills: &SkillSet, env: &Env) -> Vec<AmmoChoice> {
    let mut out: Vec<AmmoChoice> = Vec::new();
    for m in &fit.modules {
        if out.iter().any(|c| c.weapon == m.type_id) {
            continue;
        }
        let charges = dogma.compatible_charges(m.type_id);
        if charges.is_empty() || m.charge.is_none() {
            continue;
        }
        let carried = |c: TypeId| m.charge == Some(c) || fit.cargo.iter().any(|x| x.type_id == c && x.quantity > 0);
        let mut options = Vec::new();
        for c in charges {
            let mut f = fit.clone();
            for x in f.modules.iter_mut().filter(|x| x.type_id == m.type_id) {
                x.charge = Some(c);
            }
            // The weapons only: the rest of the fit's figures do not change with a charge.
            let calc = crate::calc::Calc::new(dogma, &f, skills, env);
            if let Some(w) = crate::stats::weapons(&calc, &f).into_iter().find(|w| w.type_id == m.type_id && w.charge == Some(c)) {
                options.push((c, carried(c), w));
            }
        }
        out.push(AmmoChoice { weapon: m.type_id, loaded: m.charge, options });
    }
    out
}

/// What a weapon group lands per second on the target at `distance`.
pub fn applied(w: &WeaponGroup, distance: f64, target: &TargetProfile) -> f64 {
    w.dps * applied_factor(w, distance, target, 0.0, f64::INFINITY)
}

/// The best charge at one range: (charge, applied per second), among the carried ones, and the
/// best of all when it beats that.
pub fn best_at(choice: &AmmoChoice, distance: f64, target: &TargetProfile) -> Option<((TypeId, f64), Option<(TypeId, f64)>)> {
    let best = |carried_only: bool| {
        choice.options.iter().filter(|(_, carried, _)| !carried_only || *carried).map(|(c, _, w)| (*c, applied(w, distance, target))).max_by(|a, b| a.1.total_cmp(&b.1))
    };
    let carried = best(true)?;
    let any = best(false).filter(|(c, v)| *c != carried.0 && *v > carried.1 * 1.0001);
    Some((carried, any))
}

/// A stretch of range where one carried charge is best, with a better uncarried one if any.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub from: f64,
    pub to: f64,
    pub charge: TypeId,
    /// A charge not carried that would do more over this stretch, and by how much on average
    /// over it (share).
    pub better: Option<(TypeId, f64)>,
}

/// The best carried charge over `ranges` (ascending), merged into stretches.
pub fn bands(choice: &AmmoChoice, ranges: &[f64], target: &TargetProfile) -> Vec<Band> {
    let mut out: Vec<Band> = Vec::new();
    // Per band: its points, and per uncarried charge its summed gain.
    let mut gains: Vec<(f64, Vec<(TypeId, f64)>)> = Vec::new();
    for &d in ranges {
        let Some(((charge, v), any)) = best_at(choice, d, target) else { continue };
        if v <= 0.0 {
            continue;
        }
        if out.last().is_none_or(|b| b.charge != charge) {
            out.push(Band { from: d, to: d, charge, better: None });
            gains.push((0.0, Vec::new()));
        }
        if let (Some(b), Some((n, sums))) = (out.last_mut(), gains.last_mut()) {
            b.to = d;
            *n += 1.0;
            if let Some((c, a)) = any {
                match sums.iter_mut().find(|(x, _)| *x == c) {
                    Some((_, g)) => *g += a / v - 1.0,
                    None => sums.push((c, a / v - 1.0)),
                }
            }
        }
    }
    for (b, (n, sums)) in out.iter_mut().zip(gains) {
        b.better = sums.into_iter().map(|(c, g)| (c, g / n)).filter(|(_, g)| *g > 0.01).max_by(|x, y| x.1.total_cmp(&y.1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::{CargoItem, Module, Slot, State};
    use crate::test_support::{dataset, ids};

    #[test]
    fn short_range_ammo_up_close_long_range_ammo_far_out() {
        // Short-range ammo hitting harder, as it does in the game.
        let mut d = dataset();
        let emp = d.types.iter_mut().find(|t| t.id == ids::AMMO).unwrap();
        emp.attrs.retain(|(a, _)| *a != crate::dogma::attr::EM_DAMAGE);
        emp.attrs.push((crate::dogma::attr::EM_DAMAGE, 30.0));
        emp.attrs.sort_by_key(|(a, _)| *a);
        let d = d.finish();
        let mut f = Fit::new(ids::SHIP, "x");
        f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
        f.cargo.push(CargoItem { type_id: ids::AMMO_LONG, quantity: 100, hold: None });
        let c = choices(&d, &f, &SkillSet::all_v(), &Env::default());
        let gun = c.iter().find(|x| x.weapon == ids::GUN).expect("the gun");
        assert!(gun.options.iter().any(|(c, carried, _)| *c == ids::AMMO && *carried));
        assert!(gun.options.iter().any(|(c, carried, _)| *c == ids::AMMO_LONG && *carried), "cargo ammo counts as carried");
        let target = TargetProfile { signature: 400.0, ..Default::default() };
        let ranges: Vec<f64> = (0..=60).map(|i| f64::from(i) * 1000.0).collect();
        let b = bands(gun, &ranges, &target);
        assert!(b.len() >= 2, "the best charge changes with range: {b:?}");
        assert_eq!((b.first().unwrap().charge, b.last().unwrap().charge), (ids::AMMO, ids::AMMO_LONG), "hard-hitting close, long-reaching far");
    }
}
