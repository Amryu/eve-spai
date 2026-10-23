//! EFT text, the format every fitting tool and the game's own fitting window read and write.
//!
//! ```text
//! [Rifter, Rifter fit]
//! Damage Control II
//! Gyrostabilizer II
//!
//! 1MN Afterburner II
//! [Empty Med slot]
//!
//! 200mm AutoCannon II, Republic Fleet EMP S
//! 200mm AutoCannon II, Republic Fleet EMP S /OFFLINE
//!
//! Small Projectile Burst Aerator I
//!
//!
//! Warrior II x2
//!
//! Republic Fleet EMP S x1000
//! ```
//!
//! Racks come in the order low, mid, high, rig, subsystem, service, one blank line between;
//! then drones and fighters (`Name xN`), implants and boosters, and cargo (`Name xN`). On import
//! the rack is taken from the type, not the position, so any order reads.

use crate::dogma::Dogma;
use crate::fit::{CargoItem, Fit, Slot, State};
use crate::ids::TypeId;

use super::{add_item, name_of, place_of, Place};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    pub fit: Fit,
    /// Names the data does not know, in order of appearance.
    pub unknown: Vec<String>,
}

#[derive(Debug, PartialEq)]
struct Line<'a> {
    name: &'a str,
    charge: Option<&'a str>,
    offline: bool,
    quantity: u32,
    /// The line said `xN`: it is a bay or cargo line, whatever the type.
    counted: bool,
}

/// The rack an empty-slot marker stands for: `[Empty Med slot]`.
fn empty_slot(raw: &str) -> Option<Slot> {
    let inner = raw.trim().strip_prefix('[')?.strip_suffix(']')?.to_ascii_lowercase();
    let rack = inner.strip_prefix("empty ")?.strip_suffix(" slot")?;
    Some(match rack {
        "high" => Slot::High,
        "med" | "mid" => Slot::Mid,
        "low" => Slot::Low,
        "rig" => Slot::Rig,
        "subsystem" => Slot::Subsystem,
        "service" => Slot::Service,
        _ => return None,
    })
}

fn empty_marker(slot: Slot) -> &'static str {
    match slot {
        Slot::High => "[Empty High slot]",
        Slot::Mid => "[Empty Med slot]",
        Slot::Low => "[Empty Low slot]",
        Slot::Rig => "[Empty Rig slot]",
        Slot::Subsystem => "[Empty Subsystem slot]",
        Slot::Service => "[Empty Service slot]",
    }
}

/// `Name[, Charge][ /OFFLINE][ xN][ [mutation]]`; `None` for an empty-slot marker.
fn parse_line(raw: &str) -> Option<Line<'_>> {
    let mut s = raw.trim();
    if s.starts_with('[') && s.ends_with(']') {
        return None;
    }
    let mut offline = false;
    if let Some(cut) = s.to_ascii_lowercase().rfind("/offline") {
        offline = true;
        s = s[..cut].trim_end();
    }
    if let Some(open) = s.rfind(" [") {
        if s.ends_with(']') && s[open + 2..s.len() - 1].chars().all(|c| c.is_ascii_digit()) {
            s = s[..open].trim_end();
        }
    }
    let mut quantity = 1;
    let mut counted = false;
    if let Some(x) = s.rfind(" x") {
        let digits = &s[x + 2..];
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            quantity = digits.parse().unwrap_or(1);
            counted = true;
            s = s[..x].trim_end();
        }
    }
    let (name, charge) = match s.split_once(',') {
        Some((n, c)) => (n.trim(), Some(c.trim()).filter(|c| !c.is_empty())),
        None => (s, None),
    };
    if name.is_empty() {
        return None;
    }
    Some(Line { name, charge, offline, quantity, counted })
}

pub fn import(dogma: &Dogma, text: &str) -> Option<Imported> {
    let mut lines = text.lines().map(str::trim).skip_while(|l| l.is_empty());
    let header = lines.next()?;
    let inner = header.strip_prefix('[')?.strip_suffix(']')?;
    let (ship_name, fit_name) = inner.split_once(',').map_or((inner.trim(), ""), |(s, n)| (s.trim(), n.trim()));
    let mut out = Imported::default();
    match dogma.type_named(ship_name) {
        Some(t) if place_of(dogma, t) == Place::Ship => out.fit.ship = t.id,
        _ => out.unknown.push(ship_name.to_owned()),
    }
    out.fit.name = if fit_name.is_empty() { ship_name.to_owned() } else { fit_name.to_owned() };
    // Each rack's next index, in the order written: an empty-slot marker keeps its place.
    let mut next: std::collections::HashMap<Slot, u8> = std::collections::HashMap::new();
    for raw in lines {
        if let Some(slot) = empty_slot(raw) {
            *next.entry(slot).or_default() += 1;
            continue;
        }
        let Some(line) = parse_line(raw) else { continue };
        let Some(t) = dogma.type_named(line.name) else {
            out.unknown.push(line.name.to_owned());
            continue;
        };
        let charge = line.charge.map(|c| match dogma.type_named(c) {
            Some(ct) => Some(ct.id),
            None => {
                out.unknown.push(c.to_owned());
                None
            }
        });
        match place_of(dogma, t) {
            // Spare modules travel in the cargo, written `Name xN` like everything else there;
            // fitting them would overrun the racks.
            Place::Module(_) if line.counted => out.fit.cargo.push(CargoItem { type_id: t.id, quantity: line.quantity.max(1), hold: None }),
            Place::Module(slot) => {
                for _ in 0..line.quantity.max(1) {
                    let at = next.entry(slot).or_default();
                    let index = *at;
                    *at = at.saturating_add(1);
                    out.fit.modules.push(crate::fit::Module::new(t.id, slot, index));
                    let m = out.fit.module_mut(slot, index).expect("just added");
                    m.state = if line.offline { State::Offline } else { super::default_state(dogma, t.id) };
                    m.charge = charge.flatten();
                }
            }
            Place::Other | Place::Ship => out.unknown.push(line.name.to_owned()),
            _ => {
                add_item(dogma, &mut out.fit, t.id, line.quantity);
            }
        }
    }
    super::drop_foreign_mode(dogma, &mut out.fit);
    Some(out)
}

pub fn export(dogma: &Dogma, fit: &Fit) -> String {
    export_named(fit, |id| name_of(dogma, id))
}

/// With names from elsewhere, for callers without the data at hand.
pub fn export_named(fit: &Fit, name: impl Fn(TypeId) -> String) -> String {
    let mut out = format!("[{}, {}]\n", name(fit.ship), if fit.name.is_empty() { name(fit.ship) } else { fit.name.clone() });
    let mut sections: Vec<Vec<String>> = Vec::new();
    for slot in [Slot::Low, Slot::Mid, Slot::High, Slot::Rig, Slot::Subsystem, Slot::Service] {
        // A gap between modules is written as the game writes it, so it survives a copy.
        let mut lines: Vec<String> = Vec::new();
        let mut at = 0u8;
        for m in fit.rack(slot) {
            for _ in at..m.index {
                lines.push(empty_marker(slot).to_owned());
            }
            at = m.index.saturating_add(1);
            let mut l = name(m.type_id);
            if let Some(c) = m.charge {
                l.push_str(", ");
                l.push_str(&name(c));
            }
            if m.state == State::Offline {
                l.push_str(" /OFFLINE");
            }
            lines.push(l);
        }
        if !lines.is_empty() {
            sections.push(lines);
        }
    }
    if let Some(m) = fit.mode {
        sections.push(vec![name(m)]);
    }
    let counted = |items: Vec<(TypeId, u32)>| items.into_iter().map(|(id, n)| format!("{} x{n}", name(id))).collect::<Vec<_>>();
    let bay: Vec<(TypeId, u32)> = fit.drones.iter().map(|d| (d.type_id, d.count)).chain(fit.fighters.iter().map(|f| (f.type_id, f.count))).collect();
    if !bay.is_empty() {
        sections.push(counted(bay));
    }
    let worn: Vec<String> = fit.implants.iter().copied().chain(fit.boosters.iter().map(|b| b.type_id)).map(&name).collect();
    if !worn.is_empty() {
        sections.push(worn);
    }
    if !fit.cargo.is_empty() {
        sections.push(counted(fit.cargo.iter().map(|c| (c.type_id, c.quantity)).collect()));
    }
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for l in section {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_parse_every_decoration() {
        assert_eq!(parse_line("200mm AutoCannon II, Republic Fleet EMP S /OFFLINE"), Some(Line { name: "200mm AutoCannon II", charge: Some("Republic Fleet EMP S"), offline: true, quantity: 1, counted: false }));
        assert_eq!(parse_line("Warrior II x2"), Some(Line { name: "Warrior II", charge: None, offline: false, quantity: 2, counted: true }));
        assert_eq!(parse_line("Gyrostabilizer II /offline"), Some(Line { name: "Gyrostabilizer II", charge: None, offline: true, quantity: 1, counted: false }));
        assert_eq!(parse_line("Abyssal Stasis Webifier [12]"), Some(Line { name: "Abyssal Stasis Webifier", charge: None, offline: false, quantity: 1, counted: false }));
        assert_eq!(parse_line("[Empty Low slot]"), None);
        assert_eq!(parse_line("Mjolnir Fury Light Missile x1,000"), Some(Line { name: "Mjolnir Fury Light Missile x1", charge: Some("000"), offline: false, quantity: 1, counted: false }), "a thousands separator reads as a charge, which the lookup then rejects by name");
        assert_eq!(parse_line("   "), None);
    }

    #[test]
    fn spare_modules_in_the_cargo_section_stay_in_the_cargo() {
        let d = crate::test_support::dataset();
        let text = "[Rifter, Spares]\nDamage Control II\n\n\n\n\n\nWarrior II x2\n\n200mm AutoCannon II x3\nDamage Control II x1";
        let imp = import(&d, text).expect("a fit");
        assert_eq!(imp.fit.modules.len(), 1, "only the rack line is fitted");
        assert_eq!(imp.fit.drones.len(), 1);
        assert_eq!(imp.fit.cargo.len(), 2, "counted module lines are cargo: {:?}", imp.fit.cargo);
        assert!(imp.fit.cargo.iter().any(|c| c.quantity == 3));
        let again = import(&d, &export(&d, &imp.fit)).unwrap();
        assert!(again.fit.same_fitting(&imp.fit), "and they survive a round trip");
    }
}
