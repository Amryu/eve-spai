//! A fit as the pilot built it: the hull, what sits in each slot and in what state, what is in the
//! bays, and what the pilot wears. This is the shape every store keeps (as JSON, see `doc`).

use serde::{Deserialize, Serialize};

use crate::ids::{AttrId, TypeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Slot {
    High,
    Mid,
    Low,
    Rig,
    Subsystem,
    Service,
}

impl Slot {
    pub const ALL: [Slot; 6] = [Slot::High, Slot::Mid, Slot::Low, Slot::Rig, Slot::Subsystem, Slot::Service];
}

/// A module's state, lowest first. Effects run from the state their category names upwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum State {
    Offline,
    #[default]
    Online,
    Active,
    Overload,
}

/// Attribute values an abyssal mutation gave a module, over its base type's.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mutation {
    pub mutaplasmid: TypeId,
    pub attrs: Vec<(AttrId, f32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Module {
    pub type_id: TypeId,
    pub slot: Slot,
    /// Position within the rack; with `slot` it identifies the module across edits.
    pub index: u8,
    pub state: State,
    pub charge: Option<TypeId>,
    pub mutation: Option<Mutation>,
    /// For weapons that spool up: how far along, 0..1, `None` for fully spooled.
    pub spool: Option<f32>,
}

impl Default for Module {
    fn default() -> Self {
        Module { type_id: 0, slot: Slot::High, index: 0, state: State::Online, charge: None, mutation: None, spool: None }
    }
}

impl Module {
    pub fn new(type_id: TypeId, slot: Slot, index: u8) -> Self {
        Module { type_id, slot, index, ..Module::default() }
    }

    pub fn with_charge(mut self, charge: TypeId) -> Self {
        self.charge = Some(charge);
        self
    }

    pub fn with_state(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DroneStack {
    pub type_id: TypeId,
    pub count: u32,
    /// In space, not in the bay.
    pub active: bool,
}

impl Default for DroneStack {
    fn default() -> Self {
        DroneStack { type_id: 0, count: 1, active: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FighterSquad {
    pub type_id: TypeId,
    pub count: u32,
    pub active: bool,
}

impl Default for FighterSquad {
    fn default() -> Self {
        FighterSquad { type_id: 0, count: 1, active: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Booster {
    pub type_id: TypeId,
    /// Whether its side effects are taken as rolled.
    pub side_effects: bool,
}

impl Default for Booster {
    fn default() -> Self {
        Booster { type_id: 0, side_effects: false }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CargoItem {
    pub type_id: TypeId,
    pub quantity: u32,
}

impl Default for CargoItem {
    fn default() -> Self {
        CargoItem { type_id: 0, quantity: 1 }
    }
}

/// Something another ship does to this one: a web, a painter, a remote repair.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Projected {
    pub type_id: TypeId,
    pub count: u32,
    pub state: State,
    pub charge: Option<TypeId>,
}

impl Default for Projected {
    fn default() -> Self {
        Projected { type_id: 0, count: 1, state: State::Active, charge: None }
    }
}

/// The damage split expected to come in, for effective hit points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DamageProfile {
    pub em: f32,
    pub thermal: f32,
    pub kinetic: f32,
    pub explosive: f32,
}

impl Default for DamageProfile {
    fn default() -> Self {
        DamageProfile::UNIFORM
    }
}

impl DamageProfile {
    pub const UNIFORM: DamageProfile = DamageProfile { em: 1.0, thermal: 1.0, kinetic: 1.0, explosive: 1.0 };

    /// Each share of the whole, in the order em, thermal, kinetic, explosive. A profile of all
    /// zeroes counts as uniform.
    pub fn weights(&self) -> [f64; 4] {
        let raw = [self.em, self.thermal, self.kinetic, self.explosive].map(|v| f64::from(v.max(0.0)));
        let total: f64 = raw.iter().sum();
        if total <= 0.0 {
            return [0.25; 4];
        }
        raw.map(|v| v / total)
    }
}

/// What the fit is shooting at, for applied damage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TargetProfile {
    pub signature: f32,
    pub velocity: f32,
    pub distance: f32,
    /// Metres per second across the line of sight.
    pub transversal: f32,
    pub radius: f32,
}

impl Default for TargetProfile {
    fn default() -> Self {
        TargetProfile { signature: 100.0, velocity: 0.0, distance: 10_000.0, transversal: 0.0, radius: 0.0 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fit {
    pub id: String,
    pub name: String,
    pub ship: TypeId,
    pub modules: Vec<Module>,
    pub drones: Vec<DroneStack>,
    pub fighters: Vec<FighterSquad>,
    pub implants: Vec<TypeId>,
    pub boosters: Vec<Booster>,
    pub cargo: Vec<CargoItem>,
    pub damage_profile: DamageProfile,
    pub target: TargetProfile,
    pub projected: Vec<Projected>,
    pub notes: String,
    pub tags: Vec<String>,
}

impl Fit {
    pub fn new(ship: TypeId, name: impl Into<String>) -> Self {
        Fit { ship, name: name.into(), ..Fit::default() }
    }

    pub fn module(&self, slot: Slot, index: u8) -> Option<&Module> {
        self.modules.iter().find(|m| m.slot == slot && m.index == index)
    }

    pub fn module_mut(&mut self, slot: Slot, index: u8) -> Option<&mut Module> {
        self.modules.iter_mut().find(|m| m.slot == slot && m.index == index)
    }

    /// Puts a module in the first free position of its rack and returns that position.
    pub fn add_module(&mut self, type_id: TypeId, slot: Slot) -> u8 {
        let index = (0..=u8::MAX).find(|i| self.module(slot, *i).is_none()).unwrap_or(0);
        self.modules.push(Module::new(type_id, slot, index));
        index
    }

    pub fn remove_module(&mut self, slot: Slot, index: u8) -> Option<Module> {
        let at = self.modules.iter().position(|m| m.slot == slot && m.index == index)?;
        Some(self.modules.remove(at))
    }

    pub fn rack(&self, slot: Slot) -> impl Iterator<Item = &Module> {
        let mut v: Vec<&Module> = self.modules.iter().filter(move |m| m.slot == slot).collect();
        v.sort_by_key(|m| m.index);
        v.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_are_addressed_by_rack_and_position() {
        let mut f = Fit::new(587, "test");
        assert_eq!(f.add_module(1, Slot::High), 0);
        assert_eq!(f.add_module(2, Slot::High), 1);
        assert_eq!(f.add_module(3, Slot::Low), 0);
        assert_eq!(f.remove_module(Slot::High, 0).map(|m| m.type_id), Some(1));
        assert_eq!(f.add_module(4, Slot::High), 0, "the gap is filled first");
        assert_eq!(f.rack(Slot::High).map(|m| m.type_id).collect::<Vec<_>>(), vec![4, 2]);
        assert!(f.module(Slot::Mid, 0).is_none());
    }

    #[test]
    fn a_damage_profile_normalises_and_tolerates_zeroes() {
        assert_eq!(DamageProfile::UNIFORM.weights(), [0.25; 4]);
        assert_eq!(DamageProfile { em: 2.0, thermal: 2.0, kinetic: 0.0, explosive: 0.0 }.weights(), [0.5, 0.5, 0.0, 0.0]);
        assert_eq!(DamageProfile { em: 0.0, thermal: 0.0, kinetic: 0.0, explosive: 0.0 }.weights(), [0.25; 4]);
    }

    #[test]
    fn a_fit_round_trips_as_json_and_tolerates_missing_fields() {
        let mut f = Fit::new(587, "Rifter");
        f.add_module(2048, Slot::Low);
        f.modules[0].charge = Some(1);
        f.drones.push(DroneStack { type_id: 2454, count: 2, active: true });
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<Fit>(&json).unwrap(), f);
        let old: Fit = serde_json::from_str(r#"{"ship": 587, "modules": [{"type_id": 1, "slot": "Low"}]}"#).unwrap();
        assert_eq!(old.modules[0].state, State::Online);
        assert_eq!(old.target.signature, 100.0);
    }
}
