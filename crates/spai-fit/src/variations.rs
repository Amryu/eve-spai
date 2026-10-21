//! Variations: named sets of changes kept inside a fit and switched on together, such as an
//! implant set, a long-range ammo load or an afterburner refit. A refit key links the same
//! variation across every fit of a doctrine, so a fleet swaps as one.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::dogma::Dogma;
use crate::fit::{Booster, CargoItem, DroneStack, Fit, Module, Slot, State};
use crate::ids::{GroupId, TypeId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariationKind {
    Implants,
    Boosters,
    Refit,
    Ammo,
    Drones,
    #[default]
    Custom,
}

/// Which modules a rule reaches; every set field must match.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModulePredicate {
    pub slot: Option<Slot>,
    pub group: Option<GroupId>,
    /// An effect name the type carries, such as `moduleBonusMicrowarpdrive`.
    pub effect: Option<String>,
    pub type_id: Option<TypeId>,
}

impl ModulePredicate {
    pub fn matches(&self, dogma: &Dogma, m: &Module) -> bool {
        if self.slot.is_some_and(|s| s != m.slot) || self.type_id.is_some_and(|t| t != m.type_id) {
            return false;
        }
        let Some(t) = dogma.type_(m.type_id) else { return self.group.is_none() && self.effect.is_none() };
        if self.group.is_some_and(|g| g != t.group) {
            return false;
        }
        match &self.effect {
            Some(e) => t.effects.iter().filter_map(|(id, _)| dogma.effect(*id)).any(|d| &d.name == e),
            None => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RefitOp {
    /// Put a type in the position, or empty it.
    Replace { slot: Slot, index: u8, with: Option<TypeId> },
    SetCharge { slot: Slot, index: u8, charge: Option<TypeId> },
    SetState { slot: Slot, index: u8, state: State },
    ReplaceWhere { matches: ModulePredicate, with: TypeId },
    SetChargeWhere { matches: ModulePredicate, charge: Option<TypeId> },
    SetImplants(Vec<TypeId>),
    SetBoosters(Vec<Booster>),
    SetDrones(Vec<DroneStack>),
    SetCargo(Vec<CargoItem>),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Variation {
    pub id: u32,
    pub name: String,
    pub kind: VariationKind,
    /// Shared across a doctrine: `prop:ab` on every fit's afterburner refit.
    pub refit_key: Option<String>,
    pub ops: Vec<RefitOp>,
}

/// What a variation touches, for spotting two that fight over the same thing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Footprint {
    pub positions: BTreeSet<(Slot, u8)>,
    pub implants: bool,
    pub boosters: bool,
    pub drones: bool,
    pub cargo: bool,
}

impl Footprint {
    pub fn overlaps(&self, other: &Footprint) -> bool {
        (self.implants && other.implants)
            || (self.boosters && other.boosters)
            || (self.drones && other.drones)
            || (self.cargo && other.cargo)
            || self.positions.iter().any(|p| other.positions.contains(p))
    }
}

fn apply_op(dogma: &Dogma, fit: &mut Fit, op: &RefitOp) {
    match op {
        RefitOp::Replace { slot, index, with } => {
            let old = fit.remove_module(*slot, *index);
            if let Some(t) = with {
                let mut m = Module::new(*t, *slot, *index);
                m.state = crate::formats::default_state(dogma, *t);
                if let Some(o) = &old {
                    if o.state == State::Offline {
                        m.state = State::Offline;
                    }
                    if o.charge.is_some_and(|c| crate::formats::place_of(dogma, dogma.type_(c).unwrap_or(dogma.type_(*t).unwrap_or(&dogma.types[0]))) == crate::formats::Place::Charge) {
                        m.charge = o.charge;
                    }
                }
                fit.modules.push(m);
            }
        }
        RefitOp::SetCharge { slot, index, charge } => {
            if let Some(m) = fit.module_mut(*slot, *index) {
                m.charge = *charge;
            }
        }
        RefitOp::SetState { slot, index, state } => {
            if let Some(m) = fit.module_mut(*slot, *index) {
                m.state = *state;
            }
        }
        RefitOp::ReplaceWhere { matches, with } => {
            let hits: Vec<(Slot, u8)> = fit.modules.iter().filter(|m| matches.matches(dogma, m)).map(|m| (m.slot, m.index)).collect();
            for (slot, index) in hits {
                apply_op(dogma, fit, &RefitOp::Replace { slot, index, with: Some(*with) });
            }
        }
        RefitOp::SetChargeWhere { matches, charge } => {
            for m in fit.modules.iter_mut().filter(|m| matches.matches(dogma, m)) {
                m.charge = *charge;
            }
        }
        RefitOp::SetImplants(v) => fit.implants = v.clone(),
        RefitOp::SetBoosters(v) => fit.boosters = v.clone(),
        RefitOp::SetDrones(v) => fit.drones = v.clone(),
        RefitOp::SetCargo(v) => fit.cargo = v.clone(),
    }
}

/// The fit with its active variations applied, in the order they were switched on.
pub fn resolve(dogma: &Dogma, fit: &Fit) -> Fit {
    let mut out = fit.clone();
    for id in &fit.active_variations {
        if let Some(v) = fit.variations.iter().find(|v| v.id == *id) {
            for op in &v.ops {
                apply_op(dogma, &mut out, op);
            }
        }
    }
    out
}

/// What the variation changes on this fit, rules evaluated against the fit as it is.
pub fn footprint(dogma: &Dogma, fit: &Fit, v: &Variation) -> Footprint {
    let mut f = Footprint::default();
    for op in &v.ops {
        match op {
            RefitOp::Replace { slot, index, .. } | RefitOp::SetCharge { slot, index, .. } | RefitOp::SetState { slot, index, .. } => {
                f.positions.insert((*slot, *index));
            }
            RefitOp::ReplaceWhere { matches, .. } | RefitOp::SetChargeWhere { matches, .. } => {
                f.positions.extend(fit.modules.iter().filter(|m| matches.matches(dogma, m)).map(|m| (m.slot, m.index)));
            }
            RefitOp::SetImplants(_) => f.implants = true,
            RefitOp::SetBoosters(_) => f.boosters = true,
            RefitOp::SetDrones(_) => f.drones = true,
            RefitOp::SetCargo(_) => f.cargo = true,
        }
    }
    f
}

/// Pairs of active variations that touch the same thing; the later one wins, which is worth a
/// warning.
pub fn conflicts(dogma: &Dogma, fit: &Fit) -> Vec<(u32, u32)> {
    let active: Vec<(&Variation, Footprint)> = fit.active_variations.iter().filter_map(|id| fit.variations.iter().find(|v| v.id == *id)).map(|v| (v, footprint(dogma, fit, v))).collect();
    let mut out = Vec::new();
    for (i, (a, fa)) in active.iter().enumerate() {
        for (b, fb) in &active[i + 1..] {
            if fa.overlaps(fb) {
                out.push((a.id, b.id));
            }
        }
    }
    out
}

/// The operations that turn `base` into `edited`: one per changed position or section, so a
/// variation can be recorded by editing the fit and keeping the difference.
pub fn diff(base: &Fit, edited: &Fit) -> Vec<RefitOp> {
    let mut ops = Vec::new();
    let mut positions: BTreeSet<(Slot, u8)> = base.modules.iter().chain(&edited.modules).map(|m| (m.slot, m.index)).collect();
    for (slot, index) in std::mem::take(&mut positions) {
        let (a, b) = (base.module(slot, index), edited.module(slot, index));
        match (a, b) {
            (Some(a), Some(b)) if a.type_id == b.type_id => {
                if a.charge != b.charge {
                    ops.push(RefitOp::SetCharge { slot, index, charge: b.charge });
                }
                if a.state != b.state {
                    ops.push(RefitOp::SetState { slot, index, state: b.state });
                }
            }
            (_, Some(b)) => {
                ops.push(RefitOp::Replace { slot, index, with: Some(b.type_id) });
                if b.charge.is_some() {
                    ops.push(RefitOp::SetCharge { slot, index, charge: b.charge });
                }
                if b.state != crate::fit::State::Online {
                    ops.push(RefitOp::SetState { slot, index, state: b.state });
                }
            }
            (Some(_), None) => ops.push(RefitOp::Replace { slot, index, with: None }),
            (None, None) => {}
        }
    }
    if base.implants != edited.implants {
        ops.push(RefitOp::SetImplants(edited.implants.clone()));
    }
    if base.boosters != edited.boosters {
        ops.push(RefitOp::SetBoosters(edited.boosters.clone()));
    }
    if base.drones != edited.drones {
        ops.push(RefitOp::SetDrones(edited.drones.clone()));
    }
    if base.cargo != edited.cargo {
        ops.push(RefitOp::SetCargo(edited.cargo.clone()));
    }
    ops
}

/// The part of a refit key before the colon: `prop` for `prop:ab`, which the `prop:mwd` variation
/// shares, so switching one on switches the other off.
pub fn key_family(key: &str) -> &str {
    key.split_once(':').map_or(key, |(f, _)| f)
}

/// Switches on every variation carrying the key and off every other of its family; `false` when
/// the fit has no variation with the key.
pub fn apply_refit_key(fit: &mut Fit, key: &str) -> bool {
    let family = key_family(key).to_owned();
    let ids: Vec<u32> = fit.variations.iter().filter(|v| v.refit_key.as_deref() == Some(key)).map(|v| v.id).collect();
    if ids.is_empty() {
        return false;
    }
    let same_family: Vec<u32> = fit.variations.iter().filter(|v| v.refit_key.as_deref().is_some_and(|k| key_family(k) == family)).map(|v| v.id).collect();
    fit.active_variations.retain(|id| !same_family.contains(id));
    fit.active_variations.extend(ids);
    true
}

/// Switches a variation on, and off every other of its kind and of its refit-key family: one
/// refit, one ammo load, one implant set, one booster set, one drone load at a time, so the
/// combination stays something a pilot can check the CPU and powergrid of.
pub fn activate(fit: &mut Fit, id: u32) {
    let Some(v) = fit.variations.iter().find(|v| v.id == id) else { return };
    let (kind, family) = (v.kind, v.refit_key.as_deref().map(key_family).map(str::to_owned));
    let off: Vec<u32> = fit
        .variations
        .iter()
        .filter(|o| o.id != id && (o.kind == kind || (family.is_some() && o.refit_key.as_deref().map(key_family) == family.as_deref())))
        .map(|o| o.id)
        .collect();
    fit.active_variations.retain(|a| *a != id && !off.contains(a));
    fit.active_variations.push(id);
}

pub fn deactivate(fit: &mut Fit, id: u32) {
    fit.active_variations.retain(|a| *a != id);
}

pub fn next_id(fit: &Fit) -> u32 {
    fit.variations.iter().map(|v| v.id).max().map_or(1, |m| m + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{dataset, ids};

    fn base() -> Fit {
        let mut f = Fit::new(ids::SHIP, "t");
        f.modules.push(Module::new(ids::GUN, Slot::High, 0).with_charge(ids::AMMO).with_state(State::Active));
        f.modules.push(Module::new(ids::GUN, Slot::High, 1).with_charge(ids::AMMO).with_state(State::Active));
        f.modules.push(Module::new(ids::MWD, Slot::Mid, 0).with_state(State::Active));
        f.modules.push(Module::new(ids::DC, Slot::Low, 0));
        f
    }

    #[test]
    fn a_rule_by_effect_swaps_only_the_propulsion_module() {
        let d = dataset();
        let mut f = base();
        f.variations.push(Variation {
            id: 1,
            name: "Web refit".into(),
            kind: VariationKind::Refit,
            refit_key: Some("prop:web".into()),
            ops: vec![RefitOp::ReplaceWhere { matches: ModulePredicate { effect: Some("moduleBonusMicrowarpdrive".into()), ..Default::default() }, with: ids::WEB }],
        });
        assert_eq!(resolve(&d, &f), f, "nothing active, nothing changes");
        f.active_variations.push(1);
        let r = resolve(&d, &f);
        assert_eq!(r.module(Slot::Mid, 0).map(|m| (m.type_id, m.state)), Some((ids::WEB, State::Active)));
        assert_eq!(r.modules.len(), 4);
        assert_eq!(r.module(Slot::High, 0).unwrap().charge, Some(ids::AMMO), "the guns are untouched");
        let fp = footprint(&d, &f, &f.variations[0]);
        assert_eq!(fp.positions.into_iter().collect::<Vec<_>>(), vec![(Slot::Mid, 0)]);
    }

    #[test]
    fn ammo_loads_implant_sets_and_split_guns_are_variations_too() {
        let d = dataset();
        let mut f = base();
        f.variations.push(Variation { id: 1, name: "Long range".into(), kind: VariationKind::Ammo, refit_key: None, ops: vec![RefitOp::SetChargeWhere { matches: ModulePredicate { group: Some(55), ..Default::default() }, charge: Some(ids::AMMO_LONG) }] });
        f.variations.push(Variation { id: 2, name: "Split".into(), kind: VariationKind::Ammo, refit_key: None, ops: vec![RefitOp::SetCharge { slot: Slot::High, index: 1, charge: Some(ids::AMMO_LONG) }] });
        f.variations.push(Variation { id: 3, name: "Snakes".into(), kind: VariationKind::Implants, refit_key: None, ops: vec![RefitOp::SetImplants(vec![ids::IMPLANT])] });
        f.active_variations = vec![2, 3];
        let r = resolve(&d, &f);
        assert_eq!(r.module(Slot::High, 0).unwrap().charge, Some(ids::AMMO));
        assert_eq!(r.module(Slot::High, 1).unwrap().charge, Some(ids::AMMO_LONG));
        assert_eq!(r.implants, vec![ids::IMPLANT]);
        assert!(conflicts(&d, &f).is_empty());
        f.active_variations = vec![1, 2];
        assert_eq!(conflicts(&d, &f), vec![(1, 2)], "both set the second gun's charge");
    }

    #[test]
    fn refit_keys_switch_a_family_as_one() {
        let mut f = base();
        f.variations.push(Variation { id: 1, name: "AB".into(), kind: VariationKind::Refit, refit_key: Some("prop:ab".into()), ops: vec![] });
        f.variations.push(Variation { id: 2, name: "MWD".into(), kind: VariationKind::Refit, refit_key: Some("prop:mwd".into()), ops: vec![] });
        f.variations.push(Variation { id: 3, name: "Snakes".into(), kind: VariationKind::Implants, refit_key: Some("implants:snake".into()), ops: vec![] });
        f.active_variations = vec![3, 2];
        assert!(apply_refit_key(&mut f, "prop:ab"));
        assert_eq!(f.active_variations, vec![3, 1]);
        assert!(!apply_refit_key(&mut f, "prop:nope"));
        assert_eq!(key_family("prop:ab"), "prop");
        assert_eq!(key_family("bare"), "bare");
        assert_eq!(next_id(&f), 4);
    }

    #[test]
    fn one_variation_of_each_kind_is_on_at_a_time() {
        let mut f = base();
        let v = |id, kind, key: Option<&str>| Variation { id, name: format!("v{id}"), kind, refit_key: key.map(str::to_owned), ops: vec![] };
        f.variations.push(v(1, VariationKind::Ammo, None));
        f.variations.push(v(2, VariationKind::Ammo, None));
        f.variations.push(v(3, VariationKind::Implants, None));
        f.variations.push(v(4, VariationKind::Refit, Some("prop:ab")));
        f.variations.push(v(5, VariationKind::Custom, Some("prop:mwd")));
        activate(&mut f, 1);
        activate(&mut f, 3);
        activate(&mut f, 4);
        assert_eq!(f.active_variations, vec![1, 3, 4]);
        activate(&mut f, 2);
        assert_eq!(f.active_variations, vec![3, 4, 2], "the other ammo load goes off");
        activate(&mut f, 5);
        assert_eq!(f.active_variations, vec![3, 2, 5], "the same refit-key family goes off whatever its kind");
        deactivate(&mut f, 2);
        assert_eq!(f.active_variations, vec![3, 5]);
        activate(&mut f, 99);
        assert_eq!(f.active_variations, vec![3, 5]);
    }

    #[test]
    fn a_diff_records_what_an_edit_changed_and_replays_it() {
        let d = dataset();
        let b = base();
        let mut e = b.clone();
        e.remove_module(Slot::Mid, 0);
        e.modules.push(Module::new(ids::WEB, Slot::Mid, 0).with_state(State::Active));
        e.module_mut(Slot::High, 1).unwrap().charge = Some(ids::AMMO_LONG);
        e.module_mut(Slot::Low, 0).unwrap().state = State::Offline;
        e.modules.push(Module::new(ids::REPAIRER, Slot::Low, 1).with_state(State::Active));
        e.drones.push(DroneStack { type_id: ids::DRONE, count: 2, active: true, launched: None, mutation: None });
        let ops = diff(&b, &e);
        assert_eq!(ops.len(), 7, "{ops:?}");
        let mut f = b.clone();
        f.variations.push(Variation { id: 1, name: "recorded".into(), kind: VariationKind::Custom, refit_key: None, ops });
        f.active_variations.push(1);
        let r = resolve(&d, &f);
        assert_eq!(r.modules.len(), e.modules.len());
        assert_eq!(r.module(Slot::Mid, 0).map(|m| m.type_id), Some(ids::WEB));
        assert_eq!(r.module(Slot::High, 1).unwrap().charge, Some(ids::AMMO_LONG));
        assert_eq!(r.module(Slot::Low, 0).unwrap().state, State::Offline);
        assert_eq!(r.module(Slot::Low, 1).map(|m| (m.type_id, m.state)), Some((ids::REPAIRER, State::Active)));
        assert_eq!(r.drones, e.drones);
        assert!(diff(&b, &b).is_empty());
    }

    #[test]
    fn replacing_keeps_a_charge_that_still_fits() {
        let d = dataset();
        let mut f = base();
        f.variations.push(Variation { id: 1, name: "bigger guns".into(), kind: VariationKind::Refit, refit_key: None, ops: vec![RefitOp::Replace { slot: Slot::High, index: 0, with: Some(ids::HEAVY) }, RefitOp::Replace { slot: Slot::High, index: 1, with: None }] });
        f.active_variations.push(1);
        let r = resolve(&d, &f);
        assert_eq!(r.module(Slot::High, 0).map(|m| (m.type_id, m.charge)), Some((ids::HEAVY, Some(ids::AMMO))));
        assert!(r.module(Slot::High, 1).is_none());
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<Fit>(&json).unwrap(), f);
    }
}
