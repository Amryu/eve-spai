//! Variations of the open fit: switch them on and off, record a new one by editing, link them to
//! a doctrine through a refit key.

use egui_phosphor::regular as icon;
use spai_fit::history::EditKind;
use spai_fit::variations::{self, Variation, VariationKind};
use spai_ui::widgets::icon_button;
use spai_ui::{tr, trf};

use crate::{text, SimUi};

pub fn kind_name(kind: VariationKind) -> &'static str {
    match kind {
        VariationKind::Implants => tr!("Implants"),
        VariationKind::Boosters => tr!("Boosters"),
        VariationKind::Refit => tr!("Refit"),
        VariationKind::Ammo => tr!("Ammo"),
        VariationKind::Drones => tr!("Drones"),
        VariationKind::Custom => tr!("Custom"),
    }
}

/// What the recording's changes look like, for the kind picked by default.
fn guess_kind(ops: &[variations::RefitOp]) -> VariationKind {
    use variations::RefitOp as Op;
    let all = |f: fn(&Op) -> bool| !ops.is_empty() && ops.iter().all(f);
    if all(|o| matches!(o, Op::SetImplants(_))) {
        VariationKind::Implants
    } else if all(|o| matches!(o, Op::SetBoosters(_))) {
        VariationKind::Boosters
    } else if all(|o| matches!(o, Op::SetDrones(_))) {
        VariationKind::Drones
    } else if all(|o| matches!(o, Op::SetCharge { .. } | Op::SetChargeWhere { .. } | Op::SetCargo(_))) {
        VariationKind::Ammo
    } else if all(|o| matches!(o, Op::Replace { .. } | Op::ReplaceWhere { .. } | Op::SetState { .. })) {
        VariationKind::Refit
    } else {
        VariationKind::Custom
    }
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tr!("Variations")).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if sim.recording.is_some() {
                if ui.small_button(trf!("{icon} Keep", icon = icon::CHECK)).on_hover_text(tr!("Keep the changes since the recording started as a variation")).clicked() {
                    sim.keep_recording(now_ms);
                }
                if ui.small_button(trf!("{icon} Discard", icon = icon::X)).clicked() {
                    sim.discard_recording(now_ms);
                }
            } else if sim.session.is_some() && ui.small_button(trf!("{icon} Record", icon = icon::RECORD)).on_hover_text(tr!("Edit the fit, then keep the difference as a variation")).clicked() {
                sim.start_recording(now_ms);
            }
        });
    });
    if sim.recording.is_some() {
        ui.add(egui::Label::new(egui::RichText::new(tr!("Recording: change the fit, then keep or discard.")).weak()).wrap());
    }
    ui.separator();
    let Some(s) = &sim.session else { return };
    let list: Vec<(u32, String, VariationKind, Option<String>, bool)> = s.fit.variations.iter().map(|v| (v.id, v.name.clone(), v.kind, v.refit_key.clone(), s.fit.active_variations.contains(&v.id))).collect();
    if list.is_empty() {
        ui.label(egui::RichText::new(tr!("None yet. Record one to swap ammo, implants or modules in one click.")).weak().small());
    }
    let mut toggle: Option<(u32, bool)> = None;
    let mut delete: Option<u32> = None;
    let mut rename: Option<(u32, String)> = None;
    let mut rekey: Option<(u32, String)> = None;
    let editing = sim.editing_variation;
    let mut edit_clicked: Option<Option<u32>> = None;
    for (id, name, kind, key, active) in &list {
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            let mut on = *active;
            if ui.add_enabled(sim.recording.is_none(), egui::Checkbox::without_text(&mut on)).changed() {
                toggle = Some((*id, on));
            }
            if editing == Some(*id) {
                let mut n = name.clone();
                if ui.add(egui::TextEdit::singleline(&mut n).desired_width(ui.available_width() - 60.0).hint_text(tr!("Name"))).changed() {
                    rename = Some((*id, n));
                }
                if icon_button(ui, icon::CHECK).clicked() {
                    edit_clicked = Some(None);
                }
            } else {
                let w = (ui.available_width() - 60.0).max(40.0);
                let label = format!("{} · {}", name, kind_name(*kind));
                let hover = match key {
                    Some(k) => trf!("{name}\nRefit key: {key}", name = name, key = k),
                    None => name.clone(),
                };
                crate::editor::name_cell(ui, w, egui::RichText::new(text::clip(&label, 40)), &hover);
                if icon_button(ui, icon::PENCIL_SIMPLE).on_hover_text(tr!("Rename, set the refit key")).clicked() {
                    edit_clicked = Some(Some(*id));
                }
                if icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete")).clicked() {
                    delete = Some(*id);
                }
            }
        });
        if editing == Some(*id) {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tr!("Refit key")).weak());
                let mut k = key.clone().unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut k).desired_width(ui.available_width() - 4.0).hint_text("prop:ab")).changed() {
                    rekey = Some((*id, k));
                }
            });
        }
    }
    if let Some(e) = edit_clicked {
        sim.editing_variation = e;
    }
    if let Some((id, on)) = toggle {
        sim.edit(EditKind::Variation, now_ms, |f| if on { variations::activate(f, id) } else { variations::deactivate(f, id) });
    }
    if let Some(id) = delete {
        sim.edit(EditKind::Variation, now_ms, |f| {
            f.variations.retain(|v| v.id != id);
            f.active_variations.retain(|v| *v != id);
        });
    }
    if let Some((id, n)) = rename {
        sim.edit(EditKind::Rename, now_ms, |f| {
            if let Some(v) = f.variations.iter_mut().find(|v| v.id == id) {
                v.name = n;
            }
        });
    }
    if let Some((id, k)) = rekey {
        sim.edit(EditKind::Rename, now_ms, |f| {
            if let Some(v) = f.variations.iter_mut().find(|v| v.id == id) {
                v.refit_key = Some(k.trim().to_owned()).filter(|k| !k.is_empty());
            }
        });
    }
}

impl SimUi {
    /// Starts recording: every variation goes off, so the edits that follow are against the base.
    pub fn start_recording(&mut self, now_ms: u64) {
        let Some(s) = &self.session else { return };
        let snapshot = s.fit.clone();
        self.edit(EditKind::Variation, now_ms, |f| f.active_variations.clear());
        self.recording = Some(snapshot);
    }

    /// Keeps the difference since the recording as a new, switched-on variation and puts the fit
    /// back as it was.
    pub fn keep_recording(&mut self, now_ms: u64) {
        let Some(base) = self.recording.take() else { return };
        let Some(s) = &self.session else { return };
        let mut plain = base.clone();
        plain.active_variations.clear();
        let ops = variations::diff(&plain, &s.fit);
        if ops.is_empty() {
            self.edit(EditKind::Variation, now_ms, move |f| *f = base);
            return;
        }
        let id = variations::next_id(&s.fit);
        let kind = guess_kind(&ops);
        let name = trf!("{kind} {n}", kind = kind_name(kind), n = id);
        self.edit(EditKind::Variation, now_ms, move |f| {
            *f = base;
            f.variations.push(Variation { id, name, kind, refit_key: None, ops });
            variations::activate(f, id);
        });
        self.editing_variation = Some(id);
    }

    pub fn discard_recording(&mut self, now_ms: u64) {
        if let Some(base) = self.recording.take() {
            self.edit(EditKind::Variation, now_ms, move |f| *f = base);
        }
    }

    /// The fit as it stands with its variations applied: what the numbers and the racks show.
    pub fn view_fit(&self) -> Option<spai_fit::fit::Fit> {
        let (d, s) = (self.dogma.as_ref()?, self.session.as_ref()?);
        Some(variations::resolve(d, &s.fit))
    }

    /// Rack positions an active variation decides, which the racks mark and keep read-only.
    pub fn variation_positions(&self) -> std::collections::BTreeSet<(spai_fit::fit::Slot, u8)> {
        let mut out = std::collections::BTreeSet::new();
        if let (Some(d), Some(s)) = (self.dogma.as_ref(), self.session.as_ref()) {
            for id in &s.fit.active_variations {
                if let Some(v) = s.fit.variations.iter().find(|v| v.id == *id) {
                    out.extend(variations::footprint(d, &s.fit, v).positions);
                }
            }
        }
        out
    }
}
