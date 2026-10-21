//! Where an attribute's value comes from: its base and every modifier that reached it, each with
//! the item it came from, what it did and how much of it the stacking penalty left.

use crate::dogma::Op;
use crate::ids::AttrId;

use super::value::{fraction, penalty_weights, Source};
use super::{Calc, ItemIdx};

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// The item whose attribute drives the modifier; none for a fleet buff or a patch constant.
    pub source: Option<ItemIdx>,
    pub attr: Option<AttrId>,
    pub op: Op,
    /// The modifying value as resolved: a multiplier, a percentage, an amount.
    pub value: f64,
    /// What it does to the running value: the fraction added for a multiplying operator (`0.2`
    /// for +20%), the amount for an add or subtract, the value set for an assign.
    pub effect: f64,
    pub penalised: bool,
    /// The share of `effect` that stays after the stacking penalty, 1 when none applies.
    pub weight: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Explanation {
    pub base: f64,
    pub value: f64,
    pub steps: Vec<Step>,
}

impl Step {
    /// Whether it raises the value (`None` when it changes nothing or sets it outright).
    pub fn raises(&self) -> Option<bool> {
        match self.op {
            Op::PreAssign | Op::PostAssign => None,
            Op::ModSub => (self.effect != 0.0).then_some(self.effect < 0.0),
            _ => (self.effect != 0.0).then_some(self.effect > 0.0),
        }
    }
}

impl Calc<'_> {
    /// Every modifier of the attribute on the item, with what it did, in the order they apply.
    pub fn explain(&self, item: ItemIdx, attr: AttrId) -> Explanation {
        let base = self.base(item, attr);
        let value = self.value(item, attr);
        let Some(mods) = self.mods.get(item).and_then(|m| m.get(&attr)) else { return Explanation { base, value, steps: Vec::new() } };
        let resolved: Vec<(Op, f64, bool)> = mods.iter().map(|m| (m.op, self.resolve(m.source), m.penalised)).collect();
        let weights = penalty_weights(&resolved);
        let mut steps: Vec<Step> = mods
            .iter()
            .zip(resolved.iter().zip(weights))
            .map(|(m, (&(op, v, penalised), weight))| {
                let (source, attr) = match m.source {
                    Source::Attr(i, a) => (Some(i), Some(a)),
                    Source::PropBonus(i) => (Some(i), None),
                    Source::Const(_) => (None, None),
                };
                Step { source, attr, op, value: v, effect: fraction(op, v), penalised, weight }
            })
            .collect();
        steps.sort_by_key(|s| super::value::order_of(s.op));
        Explanation { base, value, steps }
    }

    /// The attributes the item carries or has modified, sorted by id.
    pub fn attrs(&self, item: ItemIdx) -> Vec<AttrId> {
        let it = &self.items[item];
        let mut out: Vec<AttrId> = self.dogma.type_(it.type_id).map(|t| t.attrs.iter().map(|(a, _)| *a).collect()).unwrap_or_default();
        out.extend(it.overrides.iter().map(|(a, _)| *a));
        out.extend(self.mods.get(item).map(|m| m.keys().copied().collect::<Vec<_>>()).unwrap_or_default());
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Env, ItemKind, PENALTY_FACTOR};
    use crate::dogma::attr;
    use crate::fit::{Fit, Module, Slot, State};
    use crate::skills::SkillSet;
    use crate::test_support::{close, dataset, ids::*};

    #[test]
    fn every_modifier_is_named_with_its_source_and_its_share_after_the_penalty() {
        let d = dataset();
        let mut f = Fit::new(SHIP, "test");
        f.modules.push(Module::new(DC, Slot::Low, 0).with_state(State::Active));
        f.modules.push(Module::new(HARDENER, Slot::Low, 1).with_state(State::Active));
        f.modules.push(Module::new(HARDENER, Slot::Low, 2).with_state(State::Active));
        let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
        let e = c.explain(c.ship_idx(), attr::ARMOR_EM_RESONANCE);
        assert!(close(e.base, 0.4));
        assert!(close(e.value, c.ship(attr::ARMOR_EM_RESONANCE)));
        assert_eq!(e.steps.len(), 3);
        // The damage control's PreMul comes before the hardeners' PostPercent.
        assert_eq!(e.steps[0].op, Op::PreMul);
        assert!(matches!(c.items()[e.steps[0].source.unwrap()].kind, ItemKind::Module { slot: Slot::Low, index: 0 }));
        assert!(close(e.steps[0].effect, -0.15));
        assert_eq!(e.steps[0].raises(), Some(false), "a resonance going down is the value going down");
        let hardeners: Vec<&Step> = e.steps.iter().filter(|s| s.op == Op::PostPercent).collect();
        assert!(hardeners.iter().all(|s| s.penalised && close(s.effect, -0.3)));
        let mut w: Vec<f64> = hardeners.iter().map(|s| s.weight).collect();
        w.sort_by(|a, b| b.partial_cmp(a).unwrap());
        assert!(close(w[0], 1.0) && close(w[1], PENALTY_FACTOR), "the second of a kind keeps 86.9%");
        // Rebuilding the value from the steps lands on the value itself.
        let rebuilt = e.steps.iter().fold(e.base, |v, s| v * (1.0 + s.effect * s.weight));
        assert!(close(rebuilt, e.value), "{rebuilt} vs {}", e.value);
    }

    #[test]
    fn a_skill_bonus_names_the_skill_and_an_untouched_attribute_has_no_steps() {
        let d = dataset();
        let mut f = Fit::new(SHIP, "test");
        f.modules.push(Module::new(GUN, Slot::High, 0).with_state(State::Active));
        let c = Calc::new(&d, &f, &SkillSet::all_v(), &Env::default());
        let gun = c.module(Slot::High, 0).unwrap();
        let e = c.explain(gun, attr::FALLOFF);
        assert!(!e.steps.is_empty());
        assert!(e.steps.iter().any(|s| s.source.is_some_and(|i| c.items()[i].kind == ItemKind::Skill || c.items()[i].kind == ItemKind::Ship)));
        assert!(e.steps.iter().all(|s| s.raises() == Some(true)));
        assert!(c.explain(gun, attr::CPU).steps.is_empty());
        assert!(c.attrs(gun).contains(&attr::CPU));
    }
}
