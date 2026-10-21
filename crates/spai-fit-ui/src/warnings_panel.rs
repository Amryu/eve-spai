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
        WarningKind::HoldOver { hold, used, total } => trf!("{hold} overfilled ({used} of {total} m³)", hold = hold, used = text::num(*used), total = text::num(*total)),
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


/// The warnings under the fit's name: the first few, the rest behind a click; the skills one
/// opens the list.
/// The warnings in one line of fixed height: the most serious one, and a count that opens all of
/// them. With none it says so in the same space, so the racks below never move as they change.
pub fn strip(sim: &mut SimUi, ui: &mut egui::Ui) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(s) = sim.session() else { return };
    let mut lines: Vec<(Severity, bool, String)> = s.warnings.iter().map(|w| (w.severity, matches!(w.kind, WarningKind::MissingSkills { .. }), describe(&dogma, w))).collect();
    let rank = |s: Severity| match s {
        Severity::Error => 0,
        Severity::Warn => 1,
        Severity::Info => 2,
    };
    lines.sort_by_key(|(s, _, _)| rank(*s));
    let errors = lines.iter().filter(|(s, _, _)| *s == Severity::Error).count();
    let others = lines.len() - errors;
    let glyph_of = |ui: &egui::Ui, severity: Severity| match severity {
        Severity::Error => (icon::WARNING_OCTAGON, ui.visuals().error_fg_color),
        Severity::Warn => (icon::WARNING, ui.visuals().warn_fg_color),
        Severity::Info => (icon::INFO, ui.visuals().weak_text_color()),
    };
    let mut open_skills = false;
    let r = rows::row(ui, "warnings", "sim_warn_line", rows::ROW_H, false, |ui| {
        let Some((severity, skills, line)) = lines.first().cloned() else {
            ui.add(egui::Label::new(egui::RichText::new(trf!("{icon} No problems found", icon = icon::CHECK_CIRCLE)).weak()).selectable(false));
            return;
        };
        let (glyph, color) = glyph_of(ui, severity);
        ui.add(egui::Label::new(egui::RichText::new(glyph).color(color)).selectable(false));
        let count_w = if lines.len() > 1 { 150.0 } else { 0.0 };
        let w = (ui.available_width() - count_w - rows::GAP).max(60.0);
        let text = if skills { egui::RichText::new(&line).underline() } else { egui::RichText::new(&line) };
        if rows::cell(ui, w, text, &line).interact(egui::Sense::click()).clicked() && skills {
            open_skills = true;
        }
        if lines.len() > 1 {
            let label = match (errors, others) {
                (0, n) => trf!("{n} warnings", n = n),
                (1, 0) => tr!("1 error").to_owned(),
                (e, 0) => trf!("{e} errors", e = e),
                (1, n) => trf!("1 error, {n} more", n = n),
                (e, n) => trf!("{e} errors, {n} more", e = e, n = n),
            };
            let button = ui.add_sized([150.0, rows::ROW_H - 6.0], egui::Button::new(trf!("{icon} {label}", icon = icon::CARET_DOWN, label = label)).small());
            egui::Popup::menu(&button).show(|ui| {
                ui.set_min_width(360.0);
                for (severity, skills, line) in &lines {
                    let (glyph, color) = glyph_of(ui, *severity);
                    let r = ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(glyph).color(color));
                        let text = if *skills { egui::RichText::new(line).underline() } else { egui::RichText::new(line) };
                        ui.add(egui::Label::new(text).wrap().sense(egui::Sense::click()))
                    });
                    if *skills && r.inner.clicked() {
                        open_skills = true;
                    }
                }
            });
        }
    });
    let _ = r;
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
    if let Some(s) = sim.session() {
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
                    rows::num_cell(ui, 70.0, trf!("{have} to {needed}", have = have, needed = needed), false);
                });
            }
        });
    });
    sim.skills_window = open;
}
