//! Items side by side, a row each with their attributes in columns: any items, or a module's
//! whole family of variants, narrowed by meta group and level.

use std::collections::BTreeSet;

use spai_fit::ids::{AttrId, TypeId};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::details::{attrs_shown, label_of, value_text, GOOD};
use crate::SimUi;

/// What a menu asked to compare this frame, until the window takes it.
const REQUEST: &str = "sim_compare_request";

const NAME_W: f32 = 230.0;
const COL_W: f32 = 104.0;
/// An item's column when the items stand side by side: room for most of a module's name.
const ITEM_W: f32 = 150.0;

#[derive(Clone, Debug, PartialEq)]
pub struct CompareWindow {
    pub items: Vec<TypeId>,
    /// Meta groups switched off; `None` stands for items without one.
    pub hidden_meta: BTreeSet<Option<u8>>,
    /// The meta levels shown, both ends included.
    pub meta_levels: (u8, u8),
    pub all: bool,
    /// Leaves out the attributes every row has alike.
    pub only_diff: bool,
    /// The column sorted on, and whether highest first.
    pub sort: Option<(AttrId, bool)>,
    /// Items side by side as columns, an attribute a row; off, an item a row.
    pub by_attribute: bool,
    /// Attributes left out by hand.
    pub hidden_attrs: BTreeSet<AttrId>,
}

impl Default for CompareWindow {
    fn default() -> Self {
        CompareWindow { items: Vec::new(), hidden_meta: BTreeSet::new(), meta_levels: (0, u8::MAX), all: false, only_diff: true, sort: None, by_attribute: true, hidden_attrs: BTreeSet::new() }
    }
}

/// The game's name for a meta group.
pub fn meta_name(group: Option<u8>) -> String {
    match group {
        None | Some(1) => tr!("Tech I").to_owned(),
        Some(2) => tr!("Tech II").to_owned(),
        Some(3) => tr!("Storyline").to_owned(),
        Some(4) => tr!("Faction").to_owned(),
        Some(5) => tr!("Officer").to_owned(),
        Some(6) => tr!("Deadspace").to_owned(),
        Some(14) => tr!("Tech III").to_owned(),
        Some(15) => tr!("Abyssal").to_owned(),
        Some(17) => tr!("Premium").to_owned(),
        Some(19) => tr!("Limited time").to_owned(),
        Some(52) => tr!("Structure faction").to_owned(),
        Some(53) => tr!("Structure Tech II").to_owned(),
        Some(54) => tr!("Structure Tech I").to_owned(),
        Some(g) => trf!("Meta group {g}", g = g),
    }
}

/// Items without a meta group count as Tech I, as the game lists them.
fn meta_of(t: &spai_fit::dogma::TypeDef) -> Option<u8> {
    t.meta_group.filter(|g| *g != 1)
}

/// Every published variant of the type's family, its Tech I parent first.
pub fn variants(dogma: &Dogma, type_id: TypeId) -> Vec<TypeId> {
    let family: Vec<TypeId> = dogma.variants_of(type_id).iter().copied().filter(|id| dogma.type_(*id).is_some_and(|t| t.published)).collect();
    if family.is_empty() {
        vec![type_id]
    } else {
        family
    }
}

/// Asks the compare window to add these items, from a menu with no way to the `SimUi`.
pub fn request(ctx: &egui::Context, ids: &[TypeId]) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Vec<TypeId>>(egui::Id::new(REQUEST)).extend_from_slice(ids));
}

/// An item's details asked for from a menu that cannot reach the simulator, opened next frame.
const DETAILS: &str = "sim_details_request";

/// "Details…" over "Compare" and "Compare variants", for a picker's item.
pub fn menu_with_details(ui: &mut egui::Ui, dogma: &Dogma, type_id: TypeId) {
    if ui.button(tr!("Details…")).clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(DETAILS), type_id));
        ui.close();
    }
    menu(ui, dogma, type_id);
}

/// "Compare" and "Compare variants" for an item's menu.
pub fn menu(ui: &mut egui::Ui, dogma: &Dogma, type_id: TypeId) {
    if ui.button(tr!("Compare")).on_hover_text(tr!("Add it to the compare window")).clicked() {
        request(ui.ctx(), &[type_id]);
        ui.close();
    }
    let family = variants(dogma, type_id);
    if family.len() > 1 && ui.button(trf!("Compare variants ({n})", n = family.len())).on_hover_text(tr!("Add every variant of it to the compare window")).clicked() {
        request(ui.ctx(), &family);
        ui.close();
    }
}

/// The rows the filters leave, in the order shown.
pub fn visible(dogma: &Dogma, w: &CompareWindow) -> Vec<TypeId> {
    let mut out: Vec<TypeId> = w
        .items
        .iter()
        .copied()
        .filter(|id| {
            dogma.type_(*id).is_some_and(|t| {
                let level = t.meta_level.unwrap_or(0);
                !w.hidden_meta.contains(&meta_of(t)) && level >= w.meta_levels.0 && level <= w.meta_levels.1
            })
        })
        .collect();
    let value = |id: TypeId, a: AttrId| dogma.type_(id).and_then(|t| t.attr(a)).map(f64::from);
    let key = |id: &TypeId| dogma.type_(*id).map(|t| (t.meta_level.unwrap_or(0), t.name.clone())).unwrap_or_default();
    match w.sort {
        Some((a, desc)) => out.sort_by(|x, y| {
            // Rows without the attribute go last either way.
            let (vx, vy) = (value(*x, a), value(*y, a));
            match (vx, vy) {
                (Some(p), Some(q)) => if desc { q.total_cmp(&p) } else { p.total_cmp(&q) }.then_with(|| key(x).cmp(&key(y))),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => key(x).cmp(&key(y)),
            }
        }),
        None => out.sort_by_key(key),
    }
    out
}

/// The columns: every attribute the rows carry, in the order first met, without the ones all rows
/// share when `only_diff` is on.
pub fn columns(dogma: &Dogma, rows: &[TypeId], all: bool, only_diff: bool) -> Vec<AttrId> {
    let mut out: Vec<AttrId> = Vec::new();
    for id in rows {
        let Some(t) = dogma.type_(*id) else { continue };
        let attrs: Vec<AttrId> = t.attrs.iter().map(|(a, _)| *a).collect();
        for a in attrs_shown(dogma, &attrs, all) {
            if !out.contains(&a) {
                out.push(a);
            }
        }
    }
    if only_diff && rows.len() > 1 {
        out.retain(|a| {
            let vals: Vec<Option<f32>> = rows.iter().map(|id| dogma.type_(*id).and_then(|t| t.attr(*a))).collect();
            vals.iter().any(|v| *v != vals[0])
        });
    }
    out
}

/// Moves what the menus asked for into the window, opening it when needed.
fn take_requests(sim: &mut SimUi, ctx: &egui::Context) {
    if let Some(id) = ctx.data_mut(|d| d.remove_temp::<TypeId>(egui::Id::new(DETAILS))) {
        sim.open_details(crate::details::DetailsTarget::Type(id));
    }
    let asked: Vec<TypeId> = ctx.data_mut(|d| d.remove_temp::<Vec<TypeId>>(egui::Id::new(REQUEST))).unwrap_or_default();
    if asked.is_empty() {
        return;
    }
    let w = sim.compare.get_or_insert_with(CompareWindow::default);
    for id in asked {
        if !w.items.contains(&id) {
            w.items.push(id);
        }
    }
}

pub fn window(sim: &mut SimUi, ctx: &egui::Context) {
    take_requests(sim, ctx);
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(w) = sim.compare.as_mut() else { return };
    let mut open = true;
    let size = egui::vec2(900.0, 480.0);
    egui::Window::new(tr!("Compare"))
        .id(egui::Id::new("sim_compare"))
        .collapsible(false)
        .open(&mut open)
        .default_size(size)
        .min_width(480.0)
        .default_pos(ctx.content_rect().center() - size / 2.0)
        .show(ctx, |ui| body(ui, &dogma, w));
    if !open || w.items.is_empty() {
        sim.compare = None;
    }
}

fn body(ui: &mut egui::Ui, dogma: &Dogma, w: &mut CompareWindow) {
    use spai_ui::widgets::SteadySelect as _;
    // The meta groups the items come in, each a toggle once there are two.
    let mut groups: Vec<Option<u8>> = w.items.iter().filter_map(|id| dogma.type_(*id)).map(meta_of).collect();
    groups.sort();
    groups.dedup();
    let levels: Vec<u8> = w.items.iter().filter_map(|id| dogma.type_(*id)).map(|t| t.meta_level.unwrap_or(0)).collect();
    let (lo, hi) = (levels.iter().copied().min().unwrap_or(0), levels.iter().copied().max().unwrap_or(0));
    ui.horizontal_wrapped(|ui| {
        if groups.len() > 1 {
            for g in &groups {
                let on = !w.hidden_meta.contains(g);
                if ui.menu_label(on, meta_name(*g)).on_hover_text(tr!("Show or hide this meta group")).clicked() {
                    if on {
                        w.hidden_meta.insert(*g);
                    } else {
                        w.hidden_meta.remove(g);
                    }
                }
            }
            ui.separator();
        }
        if hi > lo {
            ui.label(tr!("Meta"));
            let (mut a, mut b) = (w.meta_levels.0.clamp(lo, hi), w.meta_levels.1.clamp(lo, hi));
            ui.add(egui::DragValue::new(&mut a).range(lo..=b));
            ui.label(tr!("to"));
            ui.add(egui::DragValue::new(&mut b).range(a..=hi));
            w.meta_levels = (if a == lo { 0 } else { a }, if b == hi { u8::MAX } else { b });
            ui.separator();
        }
        ui.checkbox(&mut w.only_diff, tr!("Only differences"));
        ui.checkbox(&mut w.all, tr!("All attributes"));
        ui.separator();
        if ui.menu_label(w.by_attribute, tr!("Side by side")).on_hover_text(tr!("An item a column, an attribute a row")).clicked() {
            w.by_attribute = true;
        }
        if ui.menu_label(!w.by_attribute, tr!("An item a row")).on_hover_text(tr!("An item a row, an attribute a column")).clicked() {
            w.by_attribute = false;
        }
        let shown_rows = visible(dogma, w);
        let every = columns(dogma, &shown_rows, w.all, w.only_diff);
        let hidden = every.iter().filter(|a| w.hidden_attrs.contains(a)).count();
        let label = if hidden > 0 { trf!("Attributes ({n} hidden)", n = hidden) } else { tr!("Attributes").to_owned() };
        let r = ui.button(label).on_hover_text(tr!("Pick the attributes shown"));
        egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_min_width(240.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("All")).clicked() {
                    w.hidden_attrs.clear();
                }
                if ui.button(tr!("None")).clicked() {
                    w.hidden_attrs.extend(every.iter().copied());
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().id_salt("sim_compare_attrs").max_height(360.0).show(ui, |ui| {
                for a in &every {
                    let mut on = !w.hidden_attrs.contains(a);
                    if ui.checkbox(&mut on, label_of(dogma, *a)).changed() {
                        if on {
                            w.hidden_attrs.remove(a);
                        } else {
                            w.hidden_attrs.insert(*a);
                        }
                    }
                }
            });
        });
        if ui.button(tr!("Clear")).clicked() {
            w.items.clear();
        }
    });
    ui.separator();
    let rows = visible(dogma, w);
    if rows.is_empty() {
        ui.label(egui::RichText::new(tr!("Nothing left under these filters.")).weak());
        return;
    }
    let cols: Vec<AttrId> = columns(dogma, &rows, w.all, w.only_diff).into_iter().filter(|a| !w.hidden_attrs.contains(a)).collect();
    // The best of each attribute, where the items differ.
    let best: Vec<Option<f64>> = cols
        .iter()
        .map(|a| {
            let high = dogma.attr(*a).is_none_or(|d| d.high_is_good);
            let vals: Vec<f64> = rows.iter().filter_map(|id| dogma.type_(*id).and_then(|t| t.attr(*a))).map(f64::from).collect();
            let differ = vals.iter().any(|v| (*v - vals[0]).abs() > 1e-9);
            differ.then(|| vals.iter().copied().fold(if high { f64::MIN } else { f64::MAX }, |m, v| if high { m.max(v) } else { m.min(v) }))
        })
        .collect();
    let value = |ui: &mut egui::Ui, width: f32, id: TypeId, k: usize| {
        let a = cols[k];
        match dogma.type_(id).and_then(|t| t.attr(a)).map(f64::from) {
            Some(v) => {
                let text = value_text(dogma, v, dogma.attr(a).and_then(|d| d.unit));
                match best[k] {
                    Some(b) if (v - b).abs() < 1e-9 => crate::rows::num_cell_colored(ui, width, text, GOOD),
                    _ => crate::rows::num_cell(ui, width, text, false),
                }
            }
            None => crate::rows::num_cell(ui, width, "-", true),
        }
    };
    let mut remove: Option<TypeId> = None;
    let mut sort_on: Option<AttrId> = None;
    let item_menu = |ui: &mut egui::Ui, id: TypeId, remove: &mut Option<TypeId>| {
        if ui.button(tr!("Details…")).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(DETAILS), id));
            ui.close();
        }
        if ui.button(tr!("Remove from the comparison")).clicked() {
            *remove = Some(id);
            ui.close();
        }
    };
    let gap = crate::rows::GAP;
    egui::ScrollArea::both().id_salt("sim_compare_table").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = gap;
        // Laid out at the table's own width, so the scroll area scrolls across it.
        let fixed = |ui: &mut egui::Ui, w: f32, add: &mut dyn FnMut(&mut egui::Ui) -> egui::Response| -> egui::Response {
            ui.allocate_ui_with_layout(egui::vec2(w, crate::rows::ROW_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_width(w);
                add(ui)
            })
            .inner
        };
        let sort_arrow = |a: AttrId| match w.sort {
            Some((s, true)) if s == a => egui_phosphor::regular::CARET_DOWN,
            Some((s, false)) if s == a => egui_phosphor::regular::CARET_UP,
            _ => "",
        };
        let item_name = |t: &spai_fit::dogma::TypeDef| {
            let tag = meta_name(meta_of(t));
            match t.meta_level {
                Some(m) => format!("{}  ({tag}, {m})", t.name),
                None => format!("{}  ({tag})", t.name),
            }
        };
        if w.by_attribute {
            let total = NAME_W + rows.len() as f32 * (ITEM_W + gap) + 8.0;
            fixed(ui, total, &mut |ui| {
                ui.horizontal(|ui| {
                    crate::rows::head_cell(ui, NAME_W, trf!("{n} items", n = rows.len()).as_str(), false);
                    for id in &rows {
                        let Some(t) = dogma.type_(*id) else { continue };
                        let name = item_name(t);
                        let r = ui
                            .allocate_ui_with_layout(egui::vec2(ITEM_W, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_width(ITEM_W);
                                ui.set_max_width(ITEM_W);
                                crate::icons::type_icon(ui, *id, 18.0);
                                ui.add(egui::Label::new(egui::RichText::new(&t.name).strong()).truncate().selectable(false).show_tooltip_when_elided(false));
                            })
                            .response
                            .interact(egui::Sense::click());
                        let r = r.on_hover_ui(|ui| crate::details::type_tip(ui, dogma, *id, &name));
                        r.context_menu(|ui| item_menu(ui, *id, &mut remove));
                    }
                })
                .response
            });
            for (k, a) in cols.iter().enumerate() {
                let label = label_of(dogma, *a);
                let r = fixed(ui, total, &mut |ui| {
                    crate::rows::row(ui, "compare", ("sim_compare_attr", *a), crate::rows::ROW_H, false, |ui| {
                        crate::rows::cell(ui, NAME_W - gap, egui::RichText::new(format!("{label} {}", sort_arrow(*a))), &label);
                        for id in &rows {
                            value(ui, ITEM_W, *id, k);
                        }
                    })
                });
                if r.on_hover_text(trf!("{label}: click to sort the items by it", label = label)).clicked() {
                    sort_on = Some(*a);
                }
            }
        } else {
            let total = NAME_W + cols.len() as f32 * (COL_W + gap) + 8.0;
            fixed(ui, total, &mut |ui| {
                ui.horizontal(|ui| {
                    crate::rows::head_cell(ui, NAME_W, trf!("{n} items", n = rows.len()).as_str(), false);
                    for a in &cols {
                        let label = label_of(dogma, *a);
                        let r = ui
                            .allocate_ui_with_layout(egui::vec2(COL_W, 18.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.set_min_width(COL_W);
                                ui.set_max_width(COL_W);
                                ui.add(egui::Label::new(egui::RichText::new(format!("{label} {}", sort_arrow(*a))).weak()).truncate().selectable(false).sense(egui::Sense::click()).show_tooltip_when_elided(false))
                            })
                            .inner
                            .on_hover_text(trf!("{label}: click to sort", label = label));
                        if r.clicked() {
                            sort_on = Some(*a);
                        }
                    }
                })
                .response
            });
            for id in &rows {
                let Some(t) = dogma.type_(*id) else { continue };
                let r = fixed(ui, total, &mut |ui| {
                    crate::rows::row(ui, "compare", ("sim_compare_row", *id), crate::rows::ROW_H, false, |ui| {
                        crate::icons::type_icon(ui, *id, 18.0);
                        let name = item_name(t);
                        crate::rows::cell(ui, NAME_W - 18.0 - gap, egui::RichText::new(&name), &name).on_hover_ui(|ui| crate::details::type_tip(ui, dogma, *id, ""));
                        for k in 0..cols.len() {
                            value(ui, COL_W, *id, k);
                        }
                    })
                });
                r.context_menu(|ui| item_menu(ui, *id, &mut remove));
            }
        }
    });
    if let Some(a) = sort_on {
        w.sort = match w.sort {
            Some((s, true)) if s == a => Some((a, false)),
            Some((s, false)) if s == a => None,
            _ => Some((a, true)),
        };
    }
    if let Some(id) = remove {
        w.items.retain(|x| *x != id);
    }
}
