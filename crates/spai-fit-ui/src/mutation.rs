//! Abyssal mutation of a fitted module or a drone stack: a mutaplasmid, and where each attribute it
//! rolls landed between its worst and best.

use spai_fit::fit::{Mutation, Slot};
use spai_fit::history::EditKind;
use spai_fit::ids::{AttrId, TypeId};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::{details, SimUi};

/// The name colour of a rolled item: crimson, light enough to read on the dark panels.
pub const ROLLED: egui::Color32 = egui::Color32::from_rgb(0xE8, 0x7A, 0x88);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutTarget {
    Module(Slot, u8),
    /// A drone stack, by its place in the bay.
    Drone(usize),
}

/// The item and the roll the target holds now.
fn current(sim: &SimUi, target: MutTarget) -> Option<(TypeId, Option<Mutation>)> {
    let f = &sim.session()?.fit;
    match target {
        MutTarget::Module(slot, index) => f.module(slot, index).map(|m| (m.type_id, m.mutation.clone())),
        MutTarget::Drone(i) => f.drones.get(i).map(|d| (d.type_id, d.mutation.clone())),
    }
}

/// The icon an item shows: the abyssal version's once rolled.
pub fn icon_of(dogma: &Dogma, type_id: TypeId, mutation: Option<&Mutation>) -> TypeId {
    mutation.and_then(|m| dogma.mutaplasmid(m.mutaplasmid)).and_then(|mp| mp.result_for(type_id)).unwrap_or(type_id)
}

/// The value the item's type carries for the attribute, before the roll.
fn base(dogma: &Dogma, type_id: TypeId, a: AttrId) -> f64 {
    dogma.type_(type_id).and_then(|t| t.attr(a)).map_or_else(|| dogma.attr(a).map_or(0.0, |d| f64::from(d.default)), f64::from)
}

/// A roll at `pick(min, max, high_is_good)` on every attribute the mutaplasmid rolls.
pub fn roll(dogma: &Dogma, type_id: TypeId, mutaplasmid: TypeId, pick: impl Fn(f32, f32, bool) -> f32) -> Option<Mutation> {
    let mp = dogma.mutaplasmid(mutaplasmid)?;
    let attrs = mp.attrs.iter().map(|(a, lo, hi)| (*a, (base(dogma, type_id, *a) * f64::from(pick(*lo, *hi, dogma.attr(*a).is_none_or(|d| d.high_is_good)))) as f32)).collect();
    Some(Mutation { mutaplasmid, attrs })
}

impl SimUi {
    /// Puts the roll on the target, or takes it off with `None`.
    pub fn set_mutation(&mut self, target: MutTarget, mutation: Option<Mutation>, now_ms: u64) {
        self.edit(EditKind::Other, now_ms, move |f| match target {
            MutTarget::Module(slot, index) => {
                if let Some(m) = f.module_mut(slot, index) {
                    m.mutation = mutation;
                }
            }
            MutTarget::Drone(i) => {
                if let Some(d) = f.drones.get_mut(i) {
                    d.mutation = mutation;
                }
            }
        });
    }
}

/// The dialog for the item in `mutation_window`.
pub fn window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    let Some(target) = sim.mutation_window else { return };
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some((type_id, mutation)) = current(sim, target) else {
        sim.mutation_window = None;
        return;
    };
    let name = spai_fit::formats::name_of(&dogma, type_id);
    let choices: Vec<(TypeId, String)> = dogma.mutaplasmids_for(type_id).iter().map(|m| (m.id, m.name.clone())).collect();
    let mut open = true;
    let mut set: Option<Option<Mutation>> = None;
    egui::Window::new(trf!("{name}: abyssal mutation", name = name)).id(egui::Id::new("sim_mutation")).open(&mut open).collapsible(false).resizable(false).default_width(460.0).show(ctx, |ui| {
        let chosen = mutation.as_ref().map(|m| m.mutaplasmid);
        let label = chosen.and_then(|c| choices.iter().find(|(id, _)| *id == c)).map_or_else(|| tr!("None: the item as it is").to_owned(), |(_, n)| n.clone());
        egui::ComboBox::from_id_salt("sim_mutaplasmid").width(440.0).selected_text(label).show_ui(ui, |ui| {
            if ui.selectable_label(chosen.is_none(), tr!("None: the item as it is")).clicked() {
                set = Some(None);
            }
            for (id, n) in &choices {
                if ui.selectable_label(chosen == Some(*id), n).clicked() && chosen != Some(*id) {
                    // A fresh mutaplasmid starts at no change where it can, in its range otherwise.
                    set = Some(roll(&dogma, type_id, *id, |lo, hi, _| 1.0f32.clamp(lo, hi)));
                }
            }
        });
        let Some(mu) = &mutation else {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr!("Pick a mutaplasmid to roll this item; each sets how far its attributes may move.")).weak());
            return;
        };
        let Some(mp) = dogma.mutaplasmid(mu.mutaplasmid) else { return };
        ui.add_space(6.0);
        let mut next = mu.clone();
        let mut changed = false;
        egui::Grid::new("sim_mutation_grid").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
            for (a, lo, hi) in &mp.attrs {
                let b = base(&dogma, type_id, *a);
                if b == 0.0 {
                    continue;
                }
                let def = dogma.attr(*a);
                let good_high = def.is_none_or(|d| d.high_is_good);
                let now = next.attrs.iter().find(|(x, _)| x == a).map_or(b, |(_, v)| f64::from(*v));
                let mut mult = (now / b) as f32;
                ui.label(def.map_or_else(|| format!("attribute {a}"), |d| if d.display.is_empty() { d.name.clone() } else { d.display.clone() }));
                let r = ui.add(egui::Slider::new(&mut mult, *lo..=*hi).show_value(false));
                if r.changed() {
                    let v = (b * f64::from(mult)) as f32;
                    match next.attrs.iter_mut().find(|(x, _)| x == a) {
                        Some(e) => e.1 = v,
                        None => next.attrs.push((*a, v)),
                    }
                    changed = true;
                }
                let delta = f64::from(mult) - 1.0;
                let good = (delta > 0.0) == good_high;
                let colour = if delta.abs() < 1e-6 { ui.visuals().text_color() } else if good { egui::Color32::from_rgb(0x7F, 0xC9, 0x8A) } else { ui.visuals().error_fg_color };
                ui.label(egui::RichText::new(format!("{}  {:+.1}%", details::value_text(&dogma, b * f64::from(mult), def.and_then(|d| d.unit)), delta * 100.0)).monospace().color(colour));
                ui.end_row();
            }
        });
        if changed {
            set = Some(Some(next));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let id = mu.mutaplasmid;
            if ui.button(tr!("Best roll")).clicked() {
                set = Some(roll(&dogma, type_id, id, |lo, hi, high| if high { hi } else { lo }));
            }
            if ui.button(tr!("Average")).clicked() {
                set = Some(roll(&dogma, type_id, id, |lo, hi, _| (lo + hi) / 2.0));
            }
            if ui.button(tr!("Worst roll")).clicked() {
                set = Some(roll(&dogma, type_id, id, |lo, hi, high| if high { lo } else { hi }));
            }
        });
    });
    if let Some(m) = set {
        sim.set_mutation(target, m, now_ms);
    }
    if !open {
        sim.mutation_window = None;
    }
}
