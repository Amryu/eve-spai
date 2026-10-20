//! What is wrong or missing with the fit, in a strip under its name, and the skills the pilot
//! lacks for it in a window.

use egui_phosphor::regular as icon;
use spai_fit::warnings::{DamageType, Severity, Warning, WarningKind};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::rows;
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
        WarningKind::VariationConflict { first, second } => trf!("Variations \"{first}\" and \"{second}\" change the same thing; the later one wins", first = first, second = second),
    }
}

const SHOWN: usize = 3;

/// The warnings under the fit's name: the first few, the rest behind a click; the skills one
/// opens the list.
pub fn strip(sim: &mut SimUi, ui: &mut egui::Ui) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(s) = &sim.session else { return };
    if s.warnings.is_empty() {
        return;
    }
    let total = s.warnings.len();
    let expanded = sim.warnings_expanded;
    let lines: Vec<(Severity, bool, String)> = s.warnings.iter().take(if expanded { total } else { SHOWN }).map(|w| (w.severity, matches!(w.kind, WarningKind::MissingSkills { .. }), describe(&dogma, w))).collect();
    let mut open_skills = false;
    let mut toggle = false;
    for (severity, skills, line) in lines {
        let (glyph, color) = match severity {
            Severity::Error => (icon::WARNING_OCTAGON, ui.visuals().error_fg_color),
            Severity::Warn => (icon::WARNING, ui.visuals().warn_fg_color),
            Severity::Info => (icon::INFO, ui.visuals().weak_text_color()),
        };
        let r = rows::row(ui, "warnings", ("sim_warn", &line), 20.0, false, |ui| {
            ui.label(egui::RichText::new(glyph).color(color));
            let w = ui.available_width();
            let text = if skills { egui::RichText::new(&line).underline() } else { egui::RichText::new(&line) };
            rows::cell(ui, w, text, &line);
        });
        if skills && r.clicked() {
            open_skills = true;
        }
    }
    if total > SHOWN {
        let label = if expanded { tr!("Fewer").to_owned() } else { trf!("{n} more…", n = total - SHOWN) };
        ui.horizontal(|ui| {
            ui.add_space(rows::GAP + 20.0);
            if ui.add(egui::Label::new(egui::RichText::new(label).weak().underline()).sense(egui::Sense::click())).clicked() {
                toggle = true;
            }
        });
    }
    if toggle {
        sim.warnings_expanded = !sim.warnings_expanded;
    }
    if open_skills {
        sim.skills_window = true;
    }
}

/// The skills the pilot lacks for the fit, each with the level held and the one needed.
pub fn skills_window(sim: &mut SimUi, ctx: &egui::Context) {
    if !sim.skills_window {
        return;
    }
    let Some(dogma) = sim.dogma.clone() else { return };
    let pilot = sim.profiles.get(sim.profile).map_or(String::new(), |p| p.name.clone());
    let mut by_skill: Vec<(spai_fit::ids::TypeId, u8, u8)> = Vec::new();
    if let Some(s) = &sim.session {
        for m in &s.missing {
            match by_skill.iter_mut().find(|(sk, _, _)| *sk == m.skill) {
                Some(e) => e.1 = e.1.max(m.needed),
                None => by_skill.push((m.skill, m.needed, m.have)),
            }
        }
    }
    let mut open = true;
    let size = egui::vec2(380.0, 320.0);
    egui::Window::new(trf!("Missing skills for {pilot}", pilot = pilot)).open(&mut open).collapsible(false).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        if by_skill.is_empty() {
            ui.label(egui::RichText::new(tr!("This pilot has every skill the fit needs.")).weak());
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (skill, needed, have) in by_skill {
                let name = dogma.type_(skill).map_or_else(|| trf!("Skill {id}", id = skill), |t| t.name.clone());
                rows::row(ui, "skills", ("sim_skill", skill), rows::ROW_H, false, |ui| {
                    let w = ui.available_width() - 70.0 - rows::GAP;
                    rows::cell(ui, w, egui::RichText::new(&name), &name);
                    rows::num_cell(ui, 70.0, trf!("{have} → {needed}", have = have, needed = needed), false);
                });
            }
        });
    });
    sim.skills_window = open;
}
