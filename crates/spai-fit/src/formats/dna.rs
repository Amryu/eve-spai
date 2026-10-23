//! DNA strings, `shipID:typeID;count:...::`, as the game's links carry them: subsystems first,
//! then modules, drones, fighters and charges, each with a count. No states, no charge placement.

use crate::dogma::Dogma;
use crate::fit::Fit;
use crate::ids::TypeId;

use super::{add_item, place_of, Place};

pub fn export(fit: &Fit) -> String {
    let mut parts: Vec<(TypeId, u32)> = Vec::new();
    let mut push = |id: TypeId, n: u32| match parts.iter_mut().find(|(t, _)| *t == id) {
        Some((_, q)) => *q += n,
        None => parts.push((id, n)),
    };
    for m in fit.modules.iter().filter(|m| m.slot == crate::fit::Slot::Subsystem) {
        push(m.type_id, 1);
    }
    for m in fit.modules.iter().filter(|m| m.slot != crate::fit::Slot::Subsystem) {
        push(m.type_id, 1);
    }
    if let Some(m) = fit.mode {
        push(m, 1);
    }
    for d in &fit.drones {
        push(d.type_id, d.count);
    }
    for f in &fit.fighters {
        push(f.type_id, f.count);
    }
    for c in fit.modules.iter().filter_map(|m| m.charge) {
        push(c, 1);
    }
    for c in &fit.cargo {
        push(c.type_id, c.quantity);
    }
    let body: Vec<String> = parts.iter().map(|(id, n)| format!("{id};{n}")).collect();
    format!("{}:{}::", fit.ship, body.join(":"))
}

pub fn import(dogma: &Dogma, dna: &str) -> Option<Fit> {
    let dna = dna.trim().trim_start_matches("fitting:").trim_end_matches(':');
    let mut parts = dna.split(':');
    let ship: TypeId = parts.next()?.trim().parse().ok()?;
    let ship_type = dogma.type_(ship)?;
    if place_of(dogma, ship_type) != Place::Ship {
        return None;
    }
    let mut fit = Fit::new(ship, ship_type.name.clone());
    for part in parts.filter(|p| !p.trim().is_empty()) {
        let (id, n) = part.split_once(';').unwrap_or((part, "1"));
        let (Ok(id), Ok(n)) = (id.trim().parse::<TypeId>(), n.trim().parse::<u32>()) else { continue };
        add_item(dogma, &mut fit, id, n);
    }
    super::drop_foreign_mode(dogma, &mut fit);
    Some(fit)
}
