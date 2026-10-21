//! The game's fitting XML, as its fitting window exports it: `<fittings>` holding one
//! `<fitting name="">` per fit, a `<shipType value="">` and `<hardware slot="" type="" qty="">`
//! lines. Racks as "low slot 0", "med slot 0", "hi slot 0", "rig slot 0", "subsystem slot 0",
//! "service slot 0"; "drone bay", "fighter bay" and "cargo" with a quantity. Loaded charges are
//! not part of it; they travel as cargo.

use crate::dogma::Dogma;
use crate::fit::{CargoItem, Fit, Slot};

use super::eft::Imported;
use super::{add_item, name_of, place_of, Place};

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn slot_word(slot: Slot) -> &'static str {
    match slot {
        Slot::Low => "low",
        Slot::Mid => "med",
        Slot::High => "hi",
        Slot::Rig => "rig",
        Slot::Subsystem => "subsystem",
        Slot::Service => "service",
    }
}

pub fn export(dogma: &Dogma, fits: &[&Fit]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" ?>\n<fittings>\n");
    for fit in fits {
        out += &format!("  <fitting name=\"{}\">\n    <description value=\"\"/>\n    <shipType value=\"{}\"/>\n", escape(&fit.name), escape(&name_of(dogma, fit.ship)));
        let mut modules: Vec<_> = fit.modules.iter().collect();
        modules.sort_by_key(|m| (m.slot as u8, m.index));
        for m in modules {
            out += &format!("    <hardware slot=\"{} slot {}\" type=\"{}\"/>\n", slot_word(m.slot), m.index, escape(&name_of(dogma, m.type_id)));
        }
        for d in &fit.drones {
            out += &format!("    <hardware qty=\"{}\" slot=\"drone bay\" type=\"{}\"/>\n", d.count, escape(&name_of(dogma, d.type_id)));
        }
        for f in &fit.fighters {
            out += &format!("    <hardware qty=\"{}\" slot=\"fighter bay\" type=\"{}\"/>\n", f.count, escape(&name_of(dogma, f.type_id)));
        }
        let mut cargo: Vec<(crate::ids::TypeId, u32)> = Vec::new();
        for (t, n) in fit.modules.iter().filter_map(|m| m.charge).map(|c| (c, 1)).chain(fit.cargo.iter().map(|c| (c.type_id, c.quantity))) {
            match cargo.iter_mut().find(|(x, _)| *x == t) {
                Some((_, q)) => *q += n,
                None => cargo.push((t, n)),
            }
        }
        for (t, n) in cargo {
            out += &format!("    <hardware qty=\"{n}\" slot=\"cargo\" type=\"{}\"/>\n", escape(&name_of(dogma, t)));
        }
        out += "  </fitting>\n";
    }
    out + "</fittings>\n"
}

/// The value of `name="..."` inside one tag.
fn attr_of(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let end = tag[at..].find('"')?;
    Some(unescape(&tag[at..at + end]))
}

pub fn import(dogma: &Dogma, text: &str) -> Vec<Imported> {
    let mut out = Vec::new();
    for block in text.split("<fitting ").skip(1) {
        let block = block.split("</fitting>").next().unwrap_or(block);
        let mut imp = Imported::default();
        imp.fit.name = attr_of(block, "name").unwrap_or_default();
        for tag in block.split('<').filter(|t| t.starts_with("shipType") || t.starts_with("hardware")) {
            if tag.starts_with("shipType") {
                let name = attr_of(tag, "value").unwrap_or_default();
                match dogma.type_named(&name) {
                    Some(t) if place_of(dogma, t) == Place::Ship => imp.fit.ship = t.id,
                    _ => imp.unknown.push(name),
                }
                continue;
            }
            let (Some(slot), Some(name)) = (attr_of(tag, "slot"), attr_of(tag, "type")) else { continue };
            let qty: u32 = attr_of(tag, "qty").and_then(|q| q.parse().ok()).unwrap_or(1);
            let Some(t) = dogma.type_named(&name) else {
                imp.unknown.push(name);
                continue;
            };
            if slot == "cargo" {
                imp.fit.cargo.push(CargoItem { type_id: t.id, quantity: qty.max(1), hold: None });
            } else {
                add_item(dogma, &mut imp.fit, t.id, qty);
            }
        }
        if imp.fit.name.is_empty() {
            imp.fit.name = name_of(dogma, imp.fit.ship);
        }
        out.push(imp);
    }
    out
}
