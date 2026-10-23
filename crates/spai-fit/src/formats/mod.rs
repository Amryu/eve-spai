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
    /// A hull's mode.
    Mode,
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
        _ if t.group == crate::ids::group::SHIP_MODIFIERS => Place::Mode,
        category::MODULE | category::SUBSYSTEM => dogma.slot_of(t.id).map_or(Place::Other, Place::Module),
        category::CHARGE => Place::Charge,
        category::DRONE => Place::Drone,
        category::FIGHTER => Place::Fighter,
        category::IMPLANT if t.attr(attr::BOOSTERNESS).is_some() => Place::Booster,
        category::IMPLANT => Place::Implant,
        _ => Place::Other,
    }
}

/// Whether the module can be switched on at all: an active or targeted effect other than the
/// `online` one every module carries in the Active category.
pub fn can_activate(dogma: &Dogma, type_id: TypeId) -> bool {
    dogma.type_(type_id).is_some_and(|t| {
        t.effects.iter().filter_map(|(e, _)| dogma.effect(*e)).any(|e| e.name != "online" && matches!(e.category, crate::dogma::EffectCategory::Active | crate::dogma::EffectCategory::Target))
    })
}

/// Active modules that wait online until used: cloaks, jump portals, cynos, micro jumps,
/// nullifiers, assault damage controls and signature suppressors.
const STARTS_ONLINE: [&str; 7] = ["cloaking", "jumpPortalGeneration", "cynosural", "microJump", "interdictionNullifierBonus", "moduleBonusAssaultDamageControl", "targetSpectrumBreakerBonus"];

/// A module's state when it comes in without one: active when it can be and is meant to run all
/// the time, else online.
pub fn default_state(dogma: &Dogma, type_id: TypeId) -> State {
    let waits = dogma.type_(type_id).is_some_and(|t| t.effects.iter().filter_map(|(e, _)| dogma.effect(*e)).any(|e| STARTS_ONLINE.iter().any(|p| e.name.starts_with(p))));
    if can_activate(dogma, type_id) && !waits {
        State::Active
    } else {
        State::Online
    }
}

/// Each module's state no higher than it can go: a passive module saved as active comes back
/// online, a module that cannot overheat comes back active.
pub fn clamp_states(dogma: &Dogma, fit: &mut Fit) {
    for m in &mut fit.modules {
        let overheats = dogma.type_(m.type_id).is_some_and(|t| t.attr(attr::HEAT_DAMAGE).is_some());
        let top = if !can_activate(dogma, m.type_id) { State::Online } else if overheats { State::Overload } else { State::Active };
        if m.state > top {
            m.state = top;
        }
    }
}

/// A mode the hull does not have is dropped, so it never travels with the fit.
pub fn drop_foreign_mode(dogma: &Dogma, fit: &mut Fit) {
    if fit.mode.is_some_and(|m| !dogma.modes_of(fit.ship).contains(&m)) {
        fit.mode = None;
    }
}

/// Boosters live in the cargo: the booster used in each slot is one the cargo carries. After an
/// edit (`before` the fit it started from): a booster in use that the edit took out of the cargo
/// is no longer used; one in use that the cargo never had is put in it. One booster per slot, the
/// latest picked; a slot with boosters carried and none in use uses the first of them by name.
pub fn sync_boosters(dogma: &Dogma, fit: &mut Fit, before: Option<&Fit>) {
    let slot = |t: TypeId| dogma.type_(t).and_then(|d| d.attr(attr::BOOSTERNESS)).map(|v| v as u8);
    // Any hold counts; a booster that has to be put in goes to the hull's booster bay if it has one.
    let carried = |f: &Fit, t: TypeId| f.cargo.iter().any(|c| c.type_id == t && c.quantity > 0);
    let bay = dogma.attr_named("specialBoosterHoldCapacity").map(|a| a.id).filter(|a| dogma.type_(fit.ship).and_then(|t| t.attr(*a)).is_some_and(|v| v > 0.0));
    let mut used: Vec<Booster> = Vec::new();
    for b in std::mem::take(&mut fit.boosters) {
        if !carried(fit, b.type_id) {
            if before.is_some_and(|f| carried(f, b.type_id)) {
                continue;
            }
            fit.cargo.push(CargoItem { type_id: b.type_id, quantity: 1, hold: bay });
        }
        // The latest picked for a slot wins.
        used.retain(|u| slot(u.type_id).is_none() || slot(u.type_id) != slot(b.type_id));
        used.push(b);
    }
    let mut spare: Vec<(String, TypeId)> = fit.cargo.iter().filter(|c| c.quantity > 0).map(|c| c.type_id).filter_map(|t| dogma.type_(t).filter(|d| place_of(dogma, d) == Place::Booster).map(|d| (d.name.clone(), t))).collect();
    spare.sort();
    spare.dedup();
    for (_, t) in spare {
        if slot(t).is_some() && !used.iter().any(|u| slot(u.type_id) == slot(t)) {
            used.push(Booster { type_id: t, side_effects: false, side_on: None });
        }
    }
    fit.boosters = used;
}

/// The boosters the cargo carries for the slot, by name.
pub fn boosters_for_slot(dogma: &Dogma, fit: &Fit, slot: u8) -> Vec<TypeId> {
    let mut v: Vec<(String, TypeId)> = fit.cargo.iter().filter(|c| c.quantity > 0).filter_map(|c| dogma.type_(c.type_id)).filter(|d| place_of(dogma, d) == Place::Booster && d.attr(attr::BOOSTERNESS).map(|v| v as u8) == Some(slot)).map(|d| (d.name.clone(), d.id)).collect();
    v.sort();
    v.dedup();
    v.into_iter().map(|(_, t)| t).collect()
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
        Place::Mode => fit.mode = Some(type_id),
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

/// Every fit in the text, whatever it is written in: the simulator's own document, EFT (one or
/// several), ESI's JSON (one or a
/// list, numeric or named inventory flags), the fitting XML, or a DNA string, bare, as
/// `fitting:...` or inside a game link. Empty when it is none of them.
pub fn import_any(dogma: &Dogma, text: &str) -> Vec<eft::Imported> {
    let t = text.trim();
    if t.contains("<fitting ") {
        return xml::import(dogma, t);
    }
    // The simulator's own document first: a link made here carries the fit whole, variations and
    // all. ESI's JSON reads as a document without a hull and falls through.
    if let Some(fit) = t.starts_with('{').then(|| crate::doc::from_json(t).ok()).flatten().filter(|f| f.ship != 0) {
        return vec![eft::Imported { fit, unknown: Vec::new() }];
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
