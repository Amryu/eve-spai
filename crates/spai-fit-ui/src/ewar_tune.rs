//! Fine-tuning a fleet's ewar ship by ship: how many of each slot's ships land each kind of their
//! ewar on one target, over what the fleet's plan would share out.

use spai_fit::fleet::ewar::plan_targets;
use spai_fit::fleet::{Fleet, FleetStats};
use spai_fit::stats::RemoteKind;
use spai_ui::{tr, trf};

/// Per fleet slot, the kinds tuned and how many ships' worth of each lands.
pub type SlotTunes = Vec<Vec<(RemoteKind, f32)>>;

/// The fleet's own tunes, by its slots.
pub fn of(fleet: &Fleet) -> SlotTunes {
    fleet.slots.iter().map(|s| s.ewar_tune.clone()).collect()
}

/// The fleet flying `tunes` over its plan.
pub fn apply(fleet: &mut Fleet, tunes: &SlotTunes) {
    fleet.ewar_auto = false;
    for (i, slot) in fleet.slots.iter_mut().enumerate() {
        slot.ewar_tune = tunes.get(i).cloned().unwrap_or_default();
    }
}

/// The ewar kinds a slot's ships project at the other fleet, in the order the plan offers them.
pub fn kinds(stats: &spai_fit::fleet::SlotStats) -> Vec<RemoteKind> {
    let mut out: Vec<RemoteKind> = Vec::new();
    for m in &stats.computed.remote.modules {
        if !m.kind.is_assist() && !out.contains(&m.kind) {
            out.push(m.kind);
        }
    }
    out
}

/// Whether any ship of the fleet has ewar to tune.
pub fn any(stats: &FleetStats) -> bool {
    stats.slots.iter().any(|s| s.count > 0 && !kinds(s).is_empty())
}

/// A row per ship with ewar: each kind's ships landing on one target, the plan's share until set,
/// at most as many as fly. True when one changed.
pub fn editor(ui: &mut egui::Ui, id: &str, tunes: &mut SlotTunes, fleet: &Fleet, stats: &FleetStats, labels: &[String], other_ships: u32) -> bool {
    let mut changed = false;
    tunes.resize(fleet.slots.len(), Vec::new());
    ui.push_id(("sim_ewar_tune", id), |ui| {
        for (i, s) in stats.slots.iter().enumerate().filter(|(_, s)| s.count > 0) {
            let kinds = kinds(s);
            let Some(tune) = tunes.get_mut(s.slot).filter(|_| !kinds.is_empty()) else { continue };
            let label = labels.get(i).cloned().unwrap_or_else(|| s.name.clone());
            ui.add(egui::Label::new(trf!("{n} × {ship}", n = s.count, ship = label)).truncate()).on_hover_text(label);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                for (j, kind) in kinds.into_iter().enumerate() {
                    if j > 0 {
                        ui.add_space(8.0);
                    }
                    let plan = (f64::from(s.count) / plan_targets(&fleet.ewar_plan, kind, fleet.split_fire, other_ships)) as f32;
                    let set = tune.iter().position(|(k, _)| *k == kind);
                    let mut n = set.map_or(plan, |p| tune[p].1);
                    let name = match kind {
                        RemoteKind::Scram => tr!("Tackle"),
                        k => crate::stats_panel::ewar_name(k).unwrap_or_default(),
                    };
                    let text = if set.is_some() { egui::RichText::new(name) } else { egui::RichText::new(name).weak() };
                    ui.label(text);
                    let r = ui.push_id((s.slot, j), |ui| ui.add(egui::DragValue::new(&mut n).range(0.0..=s.count as f32).speed(0.05).max_decimals(1))).inner;
                    let r = r.on_hover_text(if set.is_some() { tr!("Ships of these whose modules land on one target, set by hand; right click goes back to the plan") } else { tr!("Ships of these whose modules land on one target, as the plan shares them out; drag or type to set it") });
                    if r.changed() {
                        match set {
                            Some(p) => tune[p].1 = n,
                            None => tune.push((kind, n)),
                        }
                        changed = true;
                    }
                    if r.secondary_clicked() {
                        if let Some(p) = set {
                            tune.remove(p);
                            changed = true;
                        }
                    }
                }
            });
            ui.add_space(4.0);
        }
    });
    if tunes.iter().any(|t| !t.is_empty()) && ui.button(tr!("Back to the plan")).on_hover_text(tr!("Clear every ship's tune")).clicked() {
        tunes.iter_mut().for_each(Vec::clear);
        changed = true;
    }
    changed
}
