//! ESI's fitting document (`/characters/{id}/fittings/`): items by inventory flag.

use serde::{Deserialize, Serialize};

use crate::dogma::Dogma;
use crate::fit::{CargoItem, Fit, Slot, State};
use crate::ids::TypeId;

use super::add_item;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EsiItem {
    pub type_id: TypeId,
    pub flag: u32,
    pub quantity: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EsiFitting {
    pub name: String,
    pub description: String,
    pub ship_type_id: TypeId,
    pub items: Vec<EsiItem>,
}

pub const FLAG_CARGO: u32 = 5;
pub const FLAG_DRONE_BAY: u32 = 87;
pub const FLAG_FIGHTER_BAY: u32 = 158;

/// The inventory flag of a rack position: low 11–18, mid 19–26, high 27–34, rig 92–94,
/// subsystem 125–132, service 164–171.
pub fn flag_of(slot: Slot, index: u8) -> u32 {
    let base = match slot {
        Slot::Low => 11,
        Slot::Mid => 19,
        Slot::High => 27,
        Slot::Rig => 92,
        Slot::Subsystem => 125,
        Slot::Service => 164,
    };
    base + u32::from(index.min(7))
}

pub fn slot_of_flag(flag: u32) -> Option<(Slot, u8)> {
    let (slot, base) = match flag {
        11..=18 => (Slot::Low, 11),
        19..=26 => (Slot::Mid, 19),
        27..=34 => (Slot::High, 27),
        92..=99 => (Slot::Rig, 92),
        125..=132 => (Slot::Subsystem, 125),
        164..=171 => (Slot::Service, 164),
        _ => return None,
    };
    Some((slot, (flag - base) as u8))
}

/// Loaded charges go to cargo, as the game's own export does.
pub fn export(fit: &Fit, description: &str) -> EsiFitting {
    let mut items: Vec<EsiItem> = fit.modules.iter().map(|m| EsiItem { type_id: m.type_id, flag: flag_of(m.slot, m.index), quantity: 1 }).collect();
    items.extend(fit.drones.iter().map(|d| EsiItem { type_id: d.type_id, flag: FLAG_DRONE_BAY, quantity: d.count }));
    items.extend(fit.fighters.iter().map(|f| EsiItem { type_id: f.type_id, flag: FLAG_FIGHTER_BAY, quantity: f.count }));
    let mut cargo: Vec<(TypeId, u32)> = Vec::new();
    for (id, n) in fit.modules.iter().filter_map(|m| m.charge.map(|c| (c, 1))).chain(fit.cargo.iter().map(|c| (c.type_id, c.quantity))) {
        match cargo.iter_mut().find(|(t, _)| *t == id) {
            Some((_, q)) => *q += n,
            None => cargo.push((id, n)),
        }
    }
    items.extend(cargo.into_iter().map(|(type_id, quantity)| EsiItem { type_id, flag: FLAG_CARGO, quantity }));
    EsiFitting { name: fit.name.clone(), description: description.to_owned(), ship_type_id: fit.ship, items }
}

pub fn import(dogma: &Dogma, doc: &EsiFitting) -> Fit {
    let mut fit = Fit::new(doc.ship_type_id, doc.name.clone());
    for it in &doc.items {
        match slot_of_flag(it.flag) {
            Some((slot, index)) => {
                let index = if fit.module(slot, index).is_none() { index } else { fit.add_module(it.type_id, slot) };
                if fit.module(slot, index).is_none() {
                    fit.modules.push(crate::fit::Module::new(it.type_id, slot, index));
                }
                if let Some(m) = fit.module_mut(slot, index) {
                    m.state = super::default_state(dogma, it.type_id);
                    if slot == Slot::Rig || slot == Slot::Subsystem {
                        m.state = State::Online;
                    }
                }
            }
            None if it.flag == FLAG_CARGO => fit.cargo.push(CargoItem { type_id: it.type_id, quantity: it.quantity.max(1), hold: None }),
            None => {
                add_item(dogma, &mut fit, it.type_id, it.quantity);
            }
        }
    }
    fit
}
