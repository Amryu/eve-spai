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
    /// A reactive hardener kept at its even split instead of adapting to the incoming damage.
    pub fixed: bool,
}

impl Default for Module {
    fn default() -> Self {
        Module { type_id: 0, slot: Slot::High, index: 0, state: State::Online, charge: None, mutation: None, spool: None, fixed: false }
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
    /// How many of the stack are in space while it is out; none means all of them.
    pub launched: Option<u32>,
    /// An abyssal roll: a mutated drone, its values over its type's.
    pub mutation: Option<Mutation>,
}

impl DroneStack {
    /// How many fight: none while the stack is in the bay, else those launched.
    pub fn in_space(&self) -> u32 {
        if self.active {
            self.launched.unwrap_or(self.count).min(self.count)
        } else {
            0
        }
    }
}

impl Default for DroneStack {
    fn default() -> Self {
        DroneStack { type_id: 0, count: 1, active: true, launched: None, mutation: None }
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
    /// Which side effects are taken, by effect, when they are picked one by one; none follows
    /// `side_effects` for all of them. A booster rolls each one on its own.
    pub side_on: Option<Vec<crate::ids::EffectId>>,
}

impl Booster {
    /// Whether this one side effect applies.
    pub fn side_effect_on(&self, effect: crate::ids::EffectId) -> bool {
        match &self.side_on {
            Some(on) => on.contains(&effect),
            None => self.side_effects,
        }
    }
}

impl Default for Booster {
    fn default() -> Self {
        Booster { type_id: 0, side_effects: false, side_on: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CargoItem {
    pub type_id: TypeId,
    pub quantity: u32,
    /// The hold it sits in, by the ship attribute giving that hold's capacity; none is the cargo
    /// hold itself.
    pub hold: Option<crate::ids::AttrId>,
}

impl Default for CargoItem {
    fn default() -> Self {
        CargoItem { type_id: 0, quantity: 1, hold: None }
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
    /// From a projected fit: the module's attributes as its own fit makes them (its pilot's
    /// skills, its hull's bonuses, its charge), so they are not figured again on this one.
    #[serde(skip)]
    pub values: Vec<(crate::ids::AttrId, f64)>,
}

/// Another fit pointed at this one, by its library id, `count` ships of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectedFit {
    pub fit_id: String,
    pub count: u32,
    pub active: bool,
    /// The variation it flies with; none is its base fit.
    pub variation: Option<u32>,
}

impl Default for ProjectedFit {
    fn default() -> Self {
        ProjectedFit { fit_id: String::new(), count: 1, active: true, variation: None }
    }
}

impl Default for Projected {
    fn default() -> Self {
        Projected { type_id: 0, count: 1, state: State::Active, charge: None, values: Vec::new() }
    }
}

/// A command burst standing in for a booster fit: every charge of its kind, run from a hull (or
/// one without burst bonuses) by a pilot with these skills, with a mindlink or without.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoostShortcut {
    pub kind: crate::fleet::BurstKind,
    /// The burst module; none is the kind's Tech II.
    pub module: Option<TypeId>,
    /// The boosting hull; none is one without burst bonuses.
    pub hull: Option<TypeId>,
    /// Skill levels over all V.
    pub skills: Vec<(TypeId, u8)>,
    pub mindlink: bool,
    /// Charges switched off; all run otherwise.
    pub charges_off: Vec<TypeId>,
    pub active: bool,
}

impl Default for BoostShortcut {
    fn default() -> Self {
        BoostShortcut { kind: crate::fleet::BurstKind::Shield, module: None, hull: None, skills: Vec::new(), mindlink: false, charges_off: Vec::new(), active: true }
    }
}

/// A fit carried in a ship bay, by library id, with its hull for the room it takes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Carried {
    pub fit_id: String,
    pub ship: TypeId,
    /// The bay: the ship attribute giving its capacity.
    pub bay: AttrId,
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
    /// The hull's mode, for hulls that have them; none flies its first.
    pub mode: Option<TypeId>,
    pub modules: Vec<Module>,
    pub drones: Vec<DroneStack>,
    pub fighters: Vec<FighterSquad>,
    pub implants: Vec<TypeId>,
    pub boosters: Vec<Booster>,
    /// Implants and boosters switched off for now: kept on the fit, counted for nothing.
    pub disabled: Vec<TypeId>,
    pub cargo: Vec<CargoItem>,
    pub damage_profile: DamageProfile,
    pub target: TargetProfile,
    pub projected: Vec<Projected>,
    /// Other fits acting on this one: their remote modules and drones, and their boosts.
    pub projected_fits: Vec<ProjectedFit>,
    /// Other fits carried assembled: a frigate in the escape bay, ships in the maintenance bay.
    pub carried: Vec<Carried>,
    /// Command bursts on the fit without a booster fit of their own: a kind, a hull, skills.
    pub boosts: Vec<BoostShortcut>,
    pub variations: Vec<crate::variations::Variation>,
    /// The base's own name beside its variations; none is plain "Base".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_name: Option<String>,
    /// Switched-on variations, applied in this order.
    pub active_variations: Vec<u32>,
    pub notes: String,
    pub tags: Vec<String>,
}

impl Fit {
    /// No more drones in space than the pilot controls, all stacks together: the first stacks keep
    /// theirs, the later ones launch what is left, or stay in the bay.
    pub fn cap_drones_out(&mut self, max: u32) {
        let mut left = max;
        for d in &mut self.drones {
            let out = d.in_space().min(left);
            left -= out;
            if out != d.in_space() {
                d.active = out > 0;
                d.launched = (out > 0 && out < d.count).then_some(out);
            }
        }
    }

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

impl Fit {
    /// What is fitted, carried and worn, in a form two fits can be compared by: the hull, the
    /// modules by slot with their charges, the bays, the implants and boosters, the cargo. Not
    /// the name, the states, the variations, the target or the notes.
    pub fn fitting_key(&self) -> Vec<String> {
        let mut k: Vec<String> = vec![format!("ship {}", self.ship)];
        let mut mods: Vec<String> = self.modules.iter().map(|m| format!("{:?} {} {}", m.slot, m.type_id, m.charge.unwrap_or(0))).collect();
        mods.sort();
        k.extend(mods);
        let mut drones: Vec<String> = self.drones.iter().map(|d| format!("drone {} x{}", d.type_id, d.count)).collect();
        drones.sort();
        k.extend(drones);
        let mut fighters: Vec<String> = self.fighters.iter().map(|f| format!("fighter {} x{}", f.type_id, f.count)).collect();
        fighters.sort();
        k.extend(fighters);
        let mut worn: Vec<String> = self.implants.iter().map(|i| format!("implant {i}")).chain(self.boosters.iter().map(|b| format!("booster {}", b.type_id))).collect();
        worn.sort();
        k.extend(worn);
        let mut cargo: Vec<String> = self.cargo.iter().map(|c| format!("cargo {} x{}", c.type_id, c.quantity)).collect();
        cargo.sort();
        k.extend(cargo);
        k
    }

    /// The same hull with the same things fitted, carried and worn.
    pub fn same_fitting(&self, other: &Fit) -> bool {
        self.fitting_key() == other.fitting_key()
    }

    /// How much of the racks two fits of one hull share, 0 to 1: the modules by slot and type
    /// (charges aside) that both carry, over the larger rack. 0 for different hulls.
    pub fn rack_similarity(&self, other: &Fit) -> f64 {
        if self.ship != other.ship {
            return 0.0;
        }
        let mut mine: Vec<(Slot, TypeId)> = self.modules.iter().map(|m| (m.slot, m.type_id)).collect();
        let theirs: Vec<(Slot, TypeId)> = other.modules.iter().map(|m| (m.slot, m.type_id)).collect();
        let larger = mine.len().max(theirs.len());
        if larger == 0 {
            return 1.0;
        }
        let mut shared = 0usize;
        for t in theirs {
            if let Some(i) = mine.iter().position(|m| *m == t) {
                mine.remove(i);
                shared += 1;
            }
        }
        shared as f64 / larger as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_compare_by_what_is_fitted_and_racks_by_how_much_they_share() {
        let mut a = Fit::new(587, "A");
        a.add_module(1, Slot::High);
        a.add_module(2, Slot::High);
        a.add_module(3, Slot::Low);
        a.drones.push(DroneStack { type_id: 9, count: 2, active: true, launched: None, mutation: None });
        let mut b = a.clone();
        b.name = "B".into();
        b.modules.swap(0, 1);
        b.modules[0].state = State::Offline;
        assert!(a.same_fitting(&b), "names, order and states do not count");
        b.cargo.push(CargoItem { type_id: 5, quantity: 1, hold: None });
        assert!(!a.same_fitting(&b), "the cargo does");
        let mut c = Fit::new(587, "C");
        c.add_module(1, Slot::High);
        c.add_module(4, Slot::High);
        c.add_module(3, Slot::Low);
        assert!((a.rack_similarity(&c) - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(a.rack_similarity(&Fit::new(588, "other hull")), 0.0);
        assert_eq!(Fit::new(587, "x").rack_similarity(&Fit::new(587, "y")), 1.0);
    }

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
        f.drones.push(DroneStack { type_id: 2454, count: 2, active: true, launched: None, mutation: None });
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<Fit>(&json).unwrap(), f);
        let old: Fit = serde_json::from_str(r#"{"ship": 587, "modules": [{"type_id": 1, "slot": "Low"}]}"#).unwrap();
        assert_eq!(old.modules[0].state, State::Online);
        assert_eq!(old.target.signature, 100.0);
    }
}
