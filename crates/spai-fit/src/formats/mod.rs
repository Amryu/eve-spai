//! How fits travel: EFT text, ESI's fitting JSON, DNA strings, the game's fitting XML, multibuy
//! lists and links.

pub mod dna;
pub mod eft;
pub mod esi_json;
pub mod link;
pub mod multibuy;
pub mod xml;

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
            None => fit.drones.push(DroneStack { type_id, count: quantity.max(1), active: true, launched: None, mutation: None }),
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

/// Every fit in the text, whatever it is written in: EFT (one or several), ESI's JSON (one or a
/// list, numeric or named inventory flags), the fitting XML, or a DNA string, bare, as
/// `fitting:...` or inside a game link. Empty when it is none of them.
pub fn import_any(dogma: &Dogma, text: &str) -> Vec<eft::Imported> {
    let t = text.trim();
    if t.contains("<fitting ") {
        return xml::import(dogma, t);
    }
    if t.starts_with('{') || (t.starts_with('[') && t[1..].trim_start().starts_with('{')) {
        return esi_any(dogma, t);
    }
    if t.starts_with('[') {
        return eft_blocks(t).into_iter().filter_map(|b| eft::import(dogma, &b)).filter(|i| i.fit.ship != 0).collect();
    }
    let dna = t.find("fitting:").map_or(t, |at| &t[at + "fitting:".len()..]);
    let dna = dna.split(['>', '<']).next().unwrap_or(dna);
    dna::import(dogma, dna).map(|fit| vec![eft::Imported { fit, unknown: Vec::new() }]).unwrap_or_default()
}

/// EFT text cut at each `[Hull, Name]` header; `[Empty ... slot]` lines stay inside their fit.
fn eft_blocks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        let header = l.starts_with('[') && l.ends_with(']') && l.contains(',') && !l.to_lowercase().starts_with("[empty");
        if header || out.is_empty() {
            out.push(String::new());
        }
        let last = out.last_mut().expect("pushed");
        last.push_str(line);
        last.push('\n');
    }
    out
}

/// ESI fittings with numeric or named flags ("HiSlot0", "Cargo", "DroneBay").
fn esi_any(dogma: &Dogma, text: &str) -> Vec<eft::Imported> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else { return Vec::new() };
    let docs = match value {
        serde_json::Value::Array(a) => a,
        v => vec![v],
    };
    docs.into_iter()
        .filter_map(|mut d| {
            if let Some(items) = d.get_mut("items").and_then(|i| i.as_array_mut()) {
                for item in items {
                    if let Some(flag) = item.get("flag").and_then(|f| f.as_str()).and_then(named_flag) {
                        item["flag"] = serde_json::json!(flag);
                    }
                }
            }
            serde_json::from_value::<esi_json::EsiFitting>(d).ok()
        })
        .map(|doc| eft::Imported { fit: esi_json::import(dogma, &doc), unknown: Vec::new() })
        .filter(|i| i.fit.ship != 0)
        .collect()
}

fn named_flag(name: &str) -> Option<u32> {
    let rack = |prefix: &str, base: u32| name.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok()).map(|n| base + n);
    match name {
        "Cargo" => Some(esi_json::FLAG_CARGO),
        "DroneBay" => Some(esi_json::FLAG_DRONE_BAY),
        "FighterBay" => Some(esi_json::FLAG_FIGHTER_BAY),
        _ => rack("LoSlot", 11).or_else(|| rack("MedSlot", 19)).or_else(|| rack("HiSlot", 27)).or_else(|| rack("RigSlot", 92)).or_else(|| rack("SubSystemSlot", 125)).or_else(|| rack("ServiceSlot", 164)),
    }
}

#[cfg(test)]
mod tests;
