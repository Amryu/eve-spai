//! How fits travel: EFT text, ESI's fitting JSON and DNA strings.

pub mod dna;
pub mod eft;
pub mod esi_json;

use crate::dogma::{attr, Dogma, TypeDef};
use crate::fit::{Booster, CargoItem, DroneStack, FighterSquad, Fit, Slot, State};
use crate::ids::{category, TypeId};

/// Where an imported item goes, from its type alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Ship,
    Module(Slot),
    Charge,
    Drone,
    Fighter,
    Implant,
    Booster,
    Other,
}

pub fn place_of(dogma: &Dogma, t: &TypeDef) -> Place {
    match t.category {
        category::SHIP => Place::Ship,
        category::MODULE | category::SUBSYSTEM => dogma.slot_of(t.id).map_or(Place::Other, Place::Module),
        category::CHARGE => Place::Charge,
        category::DRONE => Place::Drone,
        category::FIGHTER => Place::Fighter,
        category::IMPLANT if t.attr(attr::BOOSTERNESS).is_some() => Place::Booster,
        category::IMPLANT => Place::Implant,
        _ => Place::Other,
    }
}

/// A module's state when it comes in without one: active when it can be, else online.
pub fn default_state(dogma: &Dogma, type_id: TypeId) -> State {
    let activatable = dogma.type_(type_id).is_some_and(|t| {
        t.attr(attr::CAPACITOR_NEED).is_some()
            || t.effects.iter().filter_map(|(e, _)| dogma.effect(*e)).any(|e| matches!(e.category, crate::dogma::EffectCategory::Active | crate::dogma::EffectCategory::Target))
    });
    if activatable {
        State::Active
    } else {
        State::Online
    }
}

/// Puts an item of the type into the fit where it belongs; `false` when there is no place.
pub fn add_item(dogma: &Dogma, fit: &mut Fit, type_id: TypeId, quantity: u32) -> bool {
    let Some(t) = dogma.type_(type_id) else { return false };
    match place_of(dogma, t) {
        Place::Ship => fit.ship = type_id,
        Place::Module(slot) => {
            for _ in 0..quantity.max(1) {
                let index = fit.add_module(type_id, slot);
                if let Some(m) = fit.module_mut(slot, index) {
                    m.state = default_state(dogma, type_id);
                }
            }
        }
        Place::Charge => fit.cargo.push(CargoItem { type_id, quantity: quantity.max(1), hold: None }),
        Place::Drone => match fit.drones.iter_mut().find(|d| d.type_id == type_id) {
            Some(d) => d.count += quantity.max(1),
            None => fit.drones.push(DroneStack { type_id, count: quantity.max(1), active: true, launched: None }),
        },
        Place::Fighter => fit.fighters.push(FighterSquad { type_id, count: quantity.max(1), active: true }),
        Place::Implant => fit.implants.push(type_id),
        Place::Booster => fit.boosters.push(Booster { type_id, side_effects: false, side_on: None }),
        Place::Other => return false,
    }
    true
}

pub fn name_of(dogma: &Dogma, type_id: TypeId) -> String {
    dogma.type_(type_id).map_or_else(|| format!("Type {type_id}"), |t| t.name.clone())
}

#[cfg(test)]
mod tests;
