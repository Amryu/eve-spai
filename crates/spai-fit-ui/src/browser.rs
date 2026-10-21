//! The module browser: the market tree as the game shows it, a search over it, and a type that
//! goes to the fit by a double click or a drag onto a slot, a bay or the hull.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use egui_phosphor::regular as icon;
use spai_fit::dogma::attr;
use spai_fit::fit::Slot;
use spai_fit::formats::{self, place_of, Place};
use spai_fit::history::EditKind;
use spai_fit::ids::{category, MarketGroupId, TypeId};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::rows::{self, DragItem, ROW_H};
use crate::{dock::DockTab, picker, text, SimUi};

const MAX_HITS: usize = 300;

/// The market tree over the types that have a place in a fit.
pub struct Tree {
    children: HashMap<Option<MarketGroupId>, Vec<MarketGroupId>>,
    types: HashMap<MarketGroupId, Vec<TypeId>>,
    names: HashMap<MarketGroupId, String>,
}

impl Tree {
    pub fn build(dogma: &Dogma) -> Self {
        let mut types: HashMap<MarketGroupId, Vec<TypeId>> = HashMap::new();
        let rolled = dogma.rolled_types();
        for t in &dogma.types {
            if rolled.contains(&t.id) {
                continue;
            }
            if !t.published || !matches!(place_of(dogma, t), Place::Ship | Place::Module(_) | Place::Charge | Place::Drone | Place::Fighter | Place::Implant | Place::Booster) {
                continue;
            }
            if let Some(g) = t.market_group {
                types.entry(g).or_default().push(t.id);
            }
        }
        for list in types.values_mut() {
            list.sort_by_cached_key(|id| (dogma.type_(*id).and_then(|t| t.meta_level).unwrap_or(0), dogma.type_(*id).map(|t| t.name.to_lowercase()).unwrap_or_default()));
        }
        // Keep a group when anything below it has types.
        let mut keep: BTreeSet<MarketGroupId> = BTreeSet::new();
        for g in types.keys() {
            let mut cur = Some(*g);
            while let Some(id) = cur {
                if !keep.insert(id) {
                    break;
                }
                cur = dogma.market_group(id).and_then(|m| m.parent);
            }
        }
        let mut children: HashMap<Option<MarketGroupId>, Vec<MarketGroupId>> = HashMap::new();
        let mut names = HashMap::new();
        for m in &dogma.market_groups {
            if keep.contains(&m.id) {
                children.entry(m.parent.filter(|p| keep.contains(p))).or_default().push(m.id);
                names.insert(m.id, m.name.clone());
            }
        }
        for list in children.values_mut() {
            list.sort_by_cached_key(|id| names.get(id).map(|n| n.to_lowercase()).unwrap_or_default());
        }
        Tree { children, types, names }
    }

    pub fn roots(&self) -> &[MarketGroupId] {
        self.children.get(&None).map_or(&[], |v| v.as_slice())
    }

    pub fn children(&self, g: MarketGroupId) -> &[MarketGroupId] {
        self.children.get(&Some(g)).map_or(&[], |v| v.as_slice())
    }

    pub fn types(&self, g: MarketGroupId) -> &[TypeId] {
        self.types.get(&g).map_or(&[], |v| v.as_slice())
    }

    pub fn name(&self, g: MarketGroupId) -> &str {
        self.names.get(&g).map_or("", |n| n.as_str())
    }

    /// The group's names from the root down.
    pub fn path(&self, dogma: &Dogma, g: MarketGroupId) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(g);
        while let Some(id) = cur {
            parts.push(self.name(id).to_owned());
            cur = dogma.market_group(id).and_then(|m| m.parent).filter(|p| self.names.contains_key(p));
        }
        parts.reverse();
        parts.join(" › ")
    }
}

#[derive(Default)]
pub struct Browser {
    pub query: String,
    pub expanded: BTreeSet<MarketGroupId>,
    tree: Option<(usize, Arc<Tree>)>,
    hits: Vec<TypeId>,
    hits_for: Option<String>,
}

impl Browser {
    /// The tree for the data at hand, built once per dataset.
    pub fn tree(&mut self, dogma: &Arc<Dogma>) -> Arc<Tree> {
        let key = Arc::as_ptr(dogma) as usize;
        match &self.tree {
            Some((k, t)) if *k == key => t.clone(),
            _ => {
                let t = Arc::new(Tree::build(dogma));
                self.tree = Some((key, t.clone()));
                t
            }
        }
    }

    /// Types whose names hold every word of the query, by name.
    pub fn search(&mut self, dogma: &Dogma) -> &[TypeId] {
        let q = self.query.trim().to_lowercase();
        if self.hits_for.as_deref() != Some(q.as_str()) {
            let words: Vec<&str> = q.split_whitespace().collect();
            let mut hits: Vec<(String, TypeId)> = if words.is_empty() {
                Vec::new()
            } else {
                let rolled = dogma.rolled_types();
                dogma
                    .types
                    .iter()
                    .filter(|t| !rolled.contains(&t.id))
                    .filter(|t| t.published && t.market_group.is_some() && matches!(place_of(dogma, t), Place::Ship | Place::Module(_) | Place::Charge | Place::Drone | Place::Fighter | Place::Implant | Place::Booster))
                    .filter(|t| {
                        let n = t.name.to_lowercase();
                        words.iter().all(|w| n.contains(w))
                    })
                    .map(|t| (t.name.to_lowercase(), t.id))
                    .collect()
            };
            hits.sort();
            self.hits = hits.into_iter().map(|(_, id)| id).take(MAX_HITS).collect();
            self.hits_for = Some(q);
        }
        &self.hits
    }
}

/// Where a dragged type can land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    Hull,
    Module(Slot, u8),
    Dock(DockTab),
}

/// The slot number an implant or booster takes, so a new one replaces the old in it.
fn wear_slot(dogma: &Dogma, type_id: TypeId) -> Option<u8> {
    let t = dogma.type_(type_id)?;
    t.attr(attr::IMPLANTNESS).or_else(|| t.attr(attr::BOOSTERNESS)).map(|v| v as u8)
}

/// Puts the type where a double click sends it: a ship becomes the hull of a tab without one and
/// goes into the cargo otherwise, a module takes the first
/// free slot of its kind, a charge loads every module that takes it, drones and fighters go to
/// their bays, an implant or booster replaces the one in its slot. `false` when it has no place.
pub fn quick_add(sim: &mut SimUi, type_id: TypeId, now_ms: u64) -> bool {
    let Some(dogma) = sim.dogma.clone() else { return false };
    let Some(t) = dogma.type_(type_id) else { return false };
    let place = place_of(&dogma, t);
    if sim.session().is_none() {
        if place == Place::Ship {
            sim.new_fit(type_id, now_ms);
            return true;
        }
        return false;
    }
    match place {
        // A hull for a tab without one; next to a hull, another ship rides in the cargo, packaged.
        Place::Ship if sim.has_hull() => add_cargo(sim, type_id, now_ms),
        Place::Ship => {
            let name = t.name.clone();
            sim.edit(EditKind::Other, now_ms, |f| {
                f.name = name;
                f.ship = type_id;
            });
        }
        Place::Module(slot) => {
            let total = sim.computed().map_or(0, |c| c.fitting.slots[Slot::ALL.iter().position(|s| *s == slot).unwrap_or(0)].1);
            let view = sim.view_fit();
            // The next free slot of its kind; a full rack takes nothing, rather than a module
            // hanging past the last slot.
            let Some(index) = (0..total as u8).find(|i| view.as_ref().is_none_or(|v| v.module(slot, *i).is_none())) else {
                sim.status(trf!("No free {slot} slot", slot = picker::slot_name(slot)));
                return false;
            };
            sim.edit(EditKind::AddModule, now_ms, |f| {
                let mut m = spai_fit::fit::Module::new(type_id, slot, index);
                m.state = formats::default_state(&dogma, type_id);
                f.modules.push(m);
            });
        }
        Place::Charge => {
            let takers: Vec<(Slot, u8)> = sim.session().map(|s| s.fit.modules.iter().filter(|m| picker::compatible_charges(&dogma, m.type_id).contains(&type_id)).map(|m| (m.slot, m.index)).collect()).unwrap_or_default();
            if takers.is_empty() {
                sim.edit(EditKind::Cargo, now_ms, |f| {
                    formats::add_item(&dogma, f, type_id, 1);
                });
            } else {
                sim.edit(EditKind::SetCharge, now_ms, |f| {
                    for (slot, index) in takers {
                        if let Some(m) = f.module_mut(slot, index) {
                            m.charge = Some(type_id);
                        }
                    }
                });
            }
        }
        Place::Drone | Place::Fighter => sim.edit(EditKind::Drones, now_ms, |f| {
            formats::add_item(&dogma, f, type_id, 1);
        }),
        Place::Implant => {
            let slot = wear_slot(&dogma, type_id);
            sim.edit(EditKind::Implants, now_ms, |f| {
                f.implants.retain(|i| slot.is_none() || wear_slot(&dogma, *i) != slot);
                f.implants.push(type_id);
            });
        }
        Place::Booster => {
            let slot = wear_slot(&dogma, type_id);
            sim.edit(EditKind::Boosters, now_ms, |f| {
                f.boosters.retain(|b| slot.is_none() || wear_slot(&dogma, b.type_id) != slot);
                f.boosters.push(spai_fit::fit::Booster { type_id, side_effects: false, side_on: None });
            });
        }
        Place::Other => return false,
    }
    true
}

/// Puts a dropped type where it was dropped, when it fits there: a ship on the hull, a module on
/// a slot of its kind (replacing what is there), a charge on a module that takes it, a drone,
/// fighter, implant, booster or charge on its bay. Implants and boosters dropped on the cargo go
/// where they belong instead. `false` when the drop made no sense.
pub fn drop_on(sim: &mut SimUi, target: DropTarget, type_id: TypeId, now_ms: u64) -> bool {
    let Some(dogma) = sim.dogma.clone() else { return false };
    let Some(t) = dogma.type_(type_id) else { return false };
    let place = place_of(&dogma, t);
    match (target, place) {
        // Dropped on the hull itself: the fit changes hull, on purpose.
        (DropTarget::Hull, Place::Ship) => {
            if sim.session().is_none() {
                sim.new_fit(type_id, now_ms);
            } else {
                sim.edit(EditKind::Other, now_ms, |f| f.ship = type_id);
            }
            true
        }
        (DropTarget::Module(slot, index), Place::Module(kind)) if kind == slot => {
            sim.edit(EditKind::AddModule, now_ms, |f| {
                f.remove_module(slot, index);
                let mut m = spai_fit::fit::Module::new(type_id, slot, index);
                m.state = formats::default_state(&dogma, type_id);
                f.modules.push(m);
            });
            true
        }
        (DropTarget::Module(slot, index), Place::Charge) => {
            let takes = sim.session().and_then(|s| s.fit.module(slot, index)).is_some_and(|m| picker::compatible_charges(&dogma, m.type_id).contains(&type_id));
            if !takes {
                return false;
            }
            sim.edit(EditKind::SetCharge, now_ms, |f| {
                if let Some(m) = f.module_mut(slot, index) {
                    m.charge = Some(type_id);
                }
            });
            true
        }
        (DropTarget::Dock(DockTab::Drones), Place::Drone) | (DropTarget::Dock(DockTab::Fighters), Place::Fighter) | (DropTarget::Dock(_), Place::Implant | Place::Booster) => quick_add(sim, type_id, now_ms),
        (DropTarget::Dock(DockTab::Projected), _) => add_projected(sim, type_id, now_ms),
        (DropTarget::Dock(DockTab::Cargo), _) => {
            add_cargo(sim, type_id, now_ms);
            true
        }
        _ => false,
    }
}

/// What can act on the fit from outside: a module or drone another ship points at it, or a
/// system effect (weather, an incursion, a beacon).
pub fn projectable(dogma: &Dogma, t: &spai_fit::dogma::TypeDef) -> bool {
    use spai_fit::dogma::EffectCategory;
    t.group == spai_fit::ids::group::EFFECT_BEACON
        || spai_fit::projection::is_generator(dogma, t.id)
        || t.category == category::DRONE
        || (t.category == category::MODULE && t.effects.iter().any(|(e, _)| dogma.effect(*e).is_some_and(|d| d.category == EffectCategory::Target)))
}

/// Puts one of the type in the cargo, whatever it is, onto a stack of it if there is one.
pub fn add_cargo(sim: &mut SimUi, type_id: TypeId, now_ms: u64) {
    // Into the hold the Cargo tab shows, unless that bay carries fits rather than items.
    let bays: Vec<spai_fit::ids::AttrId> = [spai_fit::stats::ESCAPE_BAY, spai_fit::stats::MAINTENANCE_BAY].iter().filter_map(|n| sim.dogma.as_ref().and_then(|d| d.attr_named(n)).map(|a| a.id)).collect();
    let hold = sim.dock.hold.filter(|h| !bays.contains(h));
    sim.edit(EditKind::Cargo, now_ms, |f| match f.cargo.iter_mut().find(|c| c.type_id == type_id && c.hold == hold) {
        Some(c) => c.quantity += 1,
        None => f.cargo.push(spai_fit::fit::CargoItem { type_id, quantity: 1, hold }),
    });
}

/// A system-wide effect rather than one ship's module: weather, an incursion, a phenomena
/// generator. One of a kind, on or off, never counted.
pub fn environment(dogma: &Dogma, t: &spai_fit::dogma::TypeDef) -> bool {
    t.group == spai_fit::ids::group::EFFECT_BEACON || spai_fit::projection::is_generator(dogma, t.id)
}

/// Puts a module, drone or system effect on the fit from outside; one more of it if it is there.
pub fn add_projected(sim: &mut SimUi, type_id: TypeId, now_ms: u64) -> bool {
    let Some(dogma) = sim.dogma.clone() else { return false };
    let Some(t) = dogma.type_(type_id) else { return false };
    if sim.session().is_none() || !projectable(&dogma, t) {
        return false;
    }
    let beacon = environment(&dogma, t);
    sim.edit(EditKind::Other, now_ms, |f| match f.projected.iter_mut().find(|p| p.type_id == type_id) {
        Some(_) if beacon => {}
        Some(p) => p.count += 1,
        None => f.projected.push(spai_fit::fit::Projected { type_id, ..Default::default() }),
    });
    true
}

/// Takes a drop that landed on `resp`, if any, and reports a drop that fits nowhere.
pub fn accept_drop(sim: &mut SimUi, resp: &egui::Response, target: DropTarget, now_ms: u64) {
    if let Some(item) = crate::rows::released::<DragItem>(resp) {
        if !drop_on(sim, target, item.0, now_ms) {
            let name = sim.dogma.as_ref().and_then(|d| d.type_(item.0)).map_or(String::new(), |t| t.name.clone());
            sim.note(trf!("{name} does not go there", name = name));
        }
    }
}

/// The browser column: a search box, then the tree or the matches.
pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let tree = sim.browser.tree(&dogma);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(rows::GAP);
        let w = ui.available_width() - rows::GAP;
        ui.add_sized([w, 22.0], egui::TextEdit::singleline(&mut sim.browser.query).hint_text(trf!("{icon} Search", icon = icon::MAGNIFYING_GLASS)));
    });
    ui.add_space(2.0);
    let searching = !sim.browser.query.trim().is_empty();
    let mut add: Option<TypeId> = None;
    let mut details: Option<TypeId> = None;
    egui::ScrollArea::vertical().id_salt("sim_browser").auto_shrink([false, false]).show(ui, |ui| {
        if searching {
            let hits: Vec<TypeId> = sim.browser.search(&dogma).to_vec();
            if hits.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(tr!("Nothing by that name.")).weak());
            }
            for id in hits {
                let group = dogma.type_(id).and_then(|t| t.market_group).map(|g| tree.path(&dogma, g)).unwrap_or_default();
                if type_row(ui, &dogma, id, 0.0, &group, &mut details) {
                    add = Some(id);
                }
            }
            if sim.browser.hits.len() >= MAX_HITS {
                ui.label(egui::RichText::new(trf!("First {n} matches, keep typing", n = MAX_HITS)).weak());
            }
        } else {
            let roots: Vec<MarketGroupId> = tree.roots().to_vec();
            for g in roots {
                group_rows(sim, ui, &dogma, &tree, g, 0, &mut add, &mut details);
            }
        }
    });
    if let Some(id) = add {
        if !quick_add(sim, id, now_ms) && sim.session().is_none() {
            sim.note(tr!("Pick a hull first").to_owned());
        }
    }
    if let Some(id) = details {
        sim.open_details(crate::details::DetailsTarget::Type(id));
    }
}

#[allow(clippy::too_many_arguments)]
fn group_rows(sim: &mut SimUi, ui: &mut egui::Ui, dogma: &Dogma, tree: &Tree, g: MarketGroupId, depth: usize, add: &mut Option<TypeId>, details: &mut Option<TypeId>) {
    let open = sim.browser.expanded.contains(&g);
    let indent = depth as f32 * 14.0;
    let r = rows::row(ui, "browser", ("sim_mg", g), ROW_H, false, |ui| {
        ui.add_space(indent);
        ui.add(egui::Label::new(egui::RichText::new(if open { icon::CARET_DOWN } else { icon::CARET_RIGHT }).weak()).selectable(false));
        let w = ui.available_width();
        rows::cell(ui, w, egui::RichText::new(tree.name(g)), "");
    });
    if r.clicked() {
        if open {
            sim.browser.expanded.remove(&g);
        } else {
            sim.browser.expanded.insert(g);
        }
    }
    if !open {
        return;
    }
    let children: Vec<MarketGroupId> = tree.children(g).to_vec();
    for c in children {
        group_rows(sim, ui, dogma, tree, c, depth + 1, add, details);
    }
    for id in tree.types(g).to_vec() {
        if type_row(ui, dogma, id, indent + 18.0, "", details) {
            *add = Some(id);
        }
    }
}

/// One type: an icon and its name, draggable; `true` on a double click. A right click offers
/// the details window, opened through `details`.
fn type_row(ui: &mut egui::Ui, dogma: &Dogma, id: TypeId, indent: f32, note: &str, details: &mut Option<TypeId>) -> bool {
    let Some(t) = dogma.type_(id) else { return false };
    let name = t.name.clone();
    let row = rows::row(ui, "browser", ("sim_type", id, note.len()), ROW_H, false, |ui| {
        ui.add_space(indent);
        crate::icons::type_icon(ui, id, 18.0);
        let w = ui.available_width();
        rows::cell(ui, w, egui::RichText::new(text::clip(&name, 40)), "").on_hover_ui(|ui| crate::details::type_tip(ui, dogma, id, note));
    });
    let row = row.interact(egui::Sense::drag());
    rows::drag_from(ui, &row, DragItem(id), &name);
    let mut add = row.double_clicked();
    row.context_menu(|ui| {
        if ui.button(tr!("Add to the fit")).clicked() {
            add = true;
            ui.close();
        }
        if t.category == category::SHIP && ui.button(tr!("Fit this hull")).clicked() {
            add = true;
            ui.close();
        }
        if ui.button(tr!("Details…")).clicked() {
            *details = Some(id);
            ui.close();
        }
    });
    add
}
