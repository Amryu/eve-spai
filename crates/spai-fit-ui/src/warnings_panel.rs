//! What is wrong or missing with the fit, and the skills the pilot lacks for it.

use egui_phosphor::regular as icon;
use spai_fit::warnings::{DamageType, Severity, Warning, WarningKind};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::{picker, text, SimUi};

fn damage_name(d: DamageType) -> &'static str {
    match d {
        DamageType::Em => tr!("EM"),
        DamageType::Thermal => tr!("thermal"),
        DamageType::Kinetic => tr!("kinetic"),
        DamageType::Explosive => tr!("explosive"),
    }
}

/// The warning as a sentence.
pub fn describe(dogma: &Dogma, w: &Warning) -> String {
    let name = |id| dogma.type_(id).map_or_else(|| trf!("type {id}", id = id), |t| t.name.clone());
    let at = |slot, index: u8| trf!("{slot} slot {n}", slot = picker::slot_name(slot), n = index + 1);
    match &w.kind {
        WarningKind::CpuOver { used, total } => trf!("CPU over by {n} tf ({used} of {total})", n = text::num(used - total), used = text::num(*used), total = text::num(*total)),
        WarningKind::PowerOver { used, total } => trf!("Powergrid over by {n} MW ({used} of {total})", n = text::num(used - total), used = text::num(*used), total = text::num(*total)),
        WarningKind::CalibrationOver { used, total } => trf!("Calibration over ({used} of {total})", used = text::num(*used), total = text::num(*total)),
        WarningKind::SlotsOver { slot, used, total } => trf!("{used} modules in {total} {slot} slots", used = used, total = total, slot = picker::slot_name(*slot)),
        WarningKind::TurretsOver { used, total } => trf!("{used} turrets on {total} hardpoints", used = used, total = total),
        WarningKind::LaunchersOver { used, total } => trf!("{used} launchers on {total} hardpoints", used = used, total = total),
        WarningKind::DroneBandwidthOver { used, total } => trf!("Drone bandwidth over ({used} of {total} Mbit/s)", used = text::num(*used), total = text::num(*total)),
        WarningKind::DroneBayOver { used, total } => trf!("Drone bay over ({used} of {total} m³)", used = text::num(*used), total = text::num(*total)),
        WarningKind::TooManyDronesOut { active, max } => trf!("{n} drones in space, the pilot controls {max}", n = active, max = max),
        WarningKind::EmptySlots { slot, free } => trf!("{n} empty {slot} slots", n = free, slot = picker::slot_name(*slot)),
        WarningKind::NoPropMod => tr!("No propulsion module").to_owned(),
        WarningKind::OfflineModule { slot, index } => trf!("Offline module in {at}", at = at(*slot, *index)),
        WarningKind::AncillaryArmorNoPaste { slot, index } => trf!("Ancillary armor repairer without Nanite Repair Paste in {at}", at = at(*slot, *index)),
        WarningKind::NoBoosterCharges { slot, index } => trf!("No charges loaded in {at}", at = at(*slot, *index)),
        WarningKind::NoAmmo { slot, index } => trf!("No ammunition loaded in {at}", at = at(*slot, *index)),
        WarningKind::AmmoDamageGap { weapon, missing } => trf!("{weapon}: no ammo carried for {types} damage", weapon = name(*weapon), types = missing.iter().map(|d| damage_name(*d)).collect::<Vec<_>>().join(", ")),
        WarningKind::AmmoRangeGap { weapon, short, long } => match (short, long) {
            (true, true) => trf!("{weapon}: neither short- nor long-range ammo carried", weapon = name(*weapon)),
            (true, false) => trf!("{weapon}: no short-range ammo carried", weapon = name(*weapon)),
            _ => trf!("{weapon}: no long-range ammo carried", weapon = name(*weapon)),
        },
        WarningKind::CapUnstable { lasts_s } => trf!("Capacitor lasts {t}", t = text::secs(*lasts_s)),
        WarningKind::MissingSkills { count } => trf!("{n} skills missing for this pilot", n = count),
        WarningKind::UnknownTypes(ids) => trf!("Unknown types left out: {ids}", ids = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ")),
    }
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui) {
    let Some(dogma) = sim.dogma.clone() else { return };
    sim.computed();
    let Some(s) = &sim.session else {
        ui.label(egui::RichText::new(tr!("No fit open.")).weak());
        return;
    };
    egui::ScrollArea::vertical().id_salt("sim_warnings_scroll").auto_shrink([false, false]).show(ui, |ui| {
        if s.warnings.is_empty() {
            ui.label(egui::RichText::new(tr!("Nothing to point out.")).weak());
        }
        for w in &s.warnings {
            let (glyph, color) = match w.severity {
                Severity::Error => (icon::WARNING_OCTAGON, ui.visuals().error_fg_color),
                Severity::Warn => (icon::WARNING, ui.visuals().warn_fg_color),
                Severity::Info => (icon::INFO, ui.visuals().weak_text_color()),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(glyph).color(color));
                ui.label(describe(&dogma, w));
            });
        }
        if !s.missing.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(trf!("Missing skills for {pilot}", pilot = sim.profiles.get(sim.profile).map_or("", |p| p.name.as_str()))).strong());
            let mut by_skill: Vec<(spai_fit::ids::TypeId, u8, u8)> = Vec::new();
            for m in &s.missing {
                match by_skill.iter_mut().find(|(sk, _, _)| *sk == m.skill) {
                    Some(e) => e.1 = e.1.max(m.needed),
                    None => by_skill.push((m.skill, m.needed, m.have)),
                }
            }
            for (skill, needed, have) in by_skill {
                let name = dogma.type_(skill).map_or_else(|| trf!("Skill {id}", id = skill), |t| t.name.clone());
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(&name).truncate()).on_hover_text(&name);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(trf!("{have} → {needed}", have = have, needed = needed)).monospace());
                    });
                });
            }
        }
    });
}
