//! What an item is made of: its attributes as the data has them and, once fitted, as the fit makes
//! them, with every modifier that got it there. A window per item, and the short form as a tooltip.

use spai_fit::calc::{Calc, ItemIdx, ItemKind};
use spai_fit::dogma::{attr, Op};
use spai_fit::fit::Slot;
use spai_fit::ids::{AttrId, TypeId};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::{text, SimUi};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DetailsTarget {
    /// An item as the data has it, not fitted anywhere.
    Type(TypeId),
    Ship,
    Module(Slot, u8),
    Charge(Slot, u8),
    Drone(TypeId),
    Fighter(TypeId),
    Implant(TypeId),
    Booster(TypeId),
}

/// One modifier as the tooltip tells it.
#[derive(Clone, Debug, PartialEq)]
pub struct StepRow {
    /// Who did it: "Gunnery V", "Damage Control II", "Fleet boost".
    pub who: String,
    /// What it did: "×1.20", "+5%", "+300".
    pub what: String,
    /// Good for the pilot, bad, or neither (an assignment or no change).
    pub good: Option<bool>,
    /// The share kept after the stacking penalty, when one applied.
    pub kept: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttrRow {
    pub attr: AttrId,
    pub label: String,
    pub raw: String,
    pub fitted: Option<String>,
    pub changed: bool,
    /// The fit made it better for the pilot (true) or worse (false); none when unchanged.
    pub better: Option<bool>,
    pub steps: Vec<StepRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    pub title: String,
    pub subtitle: String,
    pub fitted: bool,
    pub rows: Vec<AttrRow>,
}

pub struct DetailsWindow {
    pub target: DetailsTarget,
    pub all: bool,
    built_for: Option<(u64, usize, bool)>,
    data: Option<Details>,
}

impl DetailsWindow {
    pub fn new(target: DetailsTarget) -> Self {
        DetailsWindow { target, all: false, built_for: None, data: None }
    }
}

/// A value the fit improved, and one it made worse: soft enough to read as a tint on the number.
const GOOD: egui::Color32 = egui::Color32::from_rgb(0x7F, 0xC9, 0x8A);
const BAD: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x86, 0x7E);

/// How many attributes a tooltip shows.
const SUMMARY_ATTRS: usize = 6;

/// A type's pilot-facing attributes, those with a display name, or every one it carries.
fn attrs_shown(dogma: &Dogma, attrs: &[AttrId], all: bool) -> Vec<AttrId> {
    attrs.iter().copied().filter(|a| all || dogma.attr(*a).is_some_and(|d| !d.display.is_empty())).collect()
}

fn label_of(dogma: &Dogma, a: AttrId) -> String {
    match dogma.attr(a) {
        Some(d) if !d.display.is_empty() => d.display.clone(),
        Some(d) => d.name.clone(),
        None => format!("attribute {a}"),
    }
}

/// A value in its unit, the way the game's info window prints it.
pub fn value_text(dogma: &Dogma, v: f64, unit: Option<u8>) -> String {
    let n = |x: f64| text::num(x);
    match unit {
        Some(1) => text::metres(v),
        Some(2) => format!("{} kg", n(v)),
        Some(3) => text::secs(v),
        Some(4) => format!("{} m/s", n(v)),
        Some(9) => format!("{} m³", n(v)),
        Some(101) => text::secs(v / 1000.0),
        Some(102) => format!("{} mm", n(v)),
        Some(104) => format!("×{}", n(v)),
        Some(105) => format!("{}%", n(v)),
        Some(106) => format!("{} tf", n(v)),
        Some(107) => format!("{} MW", n(v)),
        Some(108) | Some(111) => format!("{:.1}%", (1.0 - v) * 100.0),
        Some(109) => format!("{:+.1}%", (v - 1.0) * 100.0),
        Some(112) => format!("{} rad/s", n(v)),
        Some(113) => format!("{} HP", n(v)),
        Some(114) => format!("{} GJ", n(v)),
        Some(115) => dogma.group(v as u16).map_or_else(|| format!("group {v}"), |g| g.name.clone()),
        Some(116) => dogma.type_(v as u32).map_or_else(|| format!("type {v}"), |t| t.name.clone()),
        Some(119) => label_of(dogma, v as u16),
        Some(121) | Some(124) => format!("{:+.1}%", v),
        Some(127) => format!("{:.1}%", v * 100.0),
        Some(128) => format!("{} Mbit/s", n(v)),
        Some(129) => format!("{} h", n(v)),
        Some(133) => format!("{} ISK", n(v)),
        Some(135) => format!("{} AU", n(v)),
        Some(137) => (if v != 0.0 { tr!("yes") } else { tr!("no") }).to_owned(),
        Some(140) | Some(136) | Some(141) | Some(138) | Some(122) => format!("{}", v.round() as i64),
        _ => n(v),
    }
}

/// An item's tooltip: its name, group and meta level, the first few attributes in two aligned
/// columns, then `note` (where it sits, why it is locked) and the hint to the details window.
pub fn type_tip(ui: &mut egui::Ui, dogma: &Dogma, type_id: TypeId, note: &str) {
    ui.set_max_width(320.0);
    let Some(t) = dogma.type_(type_id) else {
        ui.label(format!("type {type_id}"));
        return;
    };
    ui.label(egui::RichText::new(&t.name).strong());
    let group = dogma.group(t.group).map(|g| g.name.clone()).unwrap_or_default();
    let line = match t.meta_level {
        Some(m) if !group.is_empty() => trf!("{group} · Meta {m}", group = group, m = m),
        _ => group,
    };
    if !line.is_empty() {
        ui.label(egui::RichText::new(line).weak());
    }
    let attrs: Vec<AttrId> = t.attrs.iter().map(|(a, _)| *a).collect();
    let shown: Vec<AttrId> = attrs_shown(dogma, &attrs, false).into_iter().take(SUMMARY_ATTRS).collect();
    if !shown.is_empty() {
        ui.add_space(2.0);
        egui::Grid::new(("sim_tip", type_id)).num_columns(2).spacing([16.0, 1.0]).show(ui, |ui| {
            for a in shown {
                let v = f64::from(t.attr(a).unwrap_or(0.0));
                ui.label(egui::RichText::new(label_of(dogma, a)).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(value_text(dogma, v, dogma.attr(a).and_then(|d| d.unit))).monospace());
                });
                ui.end_row();
            }
        });
    }
    if !note.is_empty() {
        ui.add_space(2.0);
        ui.label(note);
    }
    ui.separator();
    ui.label(egui::RichText::new(tr!("Right-click for details")).weak().italics());
}

/// The item of the fit the target names, in a calculation built for it.
fn item_of(calc: &Calc, fit: &spai_fit::fit::Fit, target: DetailsTarget) -> Option<ItemIdx> {
    let by_type = |want: TypeId, pick: &dyn Fn(ItemKind) -> bool| calc.items().iter().position(|i| i.type_id == want && pick(i.kind));
    match target {
        DetailsTarget::Type(_) => None,
        DetailsTarget::Ship => Some(calc.ship_idx()),
        DetailsTarget::Module(slot, index) => calc.module(slot, index),
        DetailsTarget::Charge(slot, index) => calc.module(slot, index).and_then(|m| calc.items()[m].charge),
        DetailsTarget::Drone(t) => by_type(t, &|k| matches!(k, ItemKind::Drone { .. })),
        DetailsTarget::Fighter(t) => by_type(t, &|k| matches!(k, ItemKind::Fighter { .. })),
        DetailsTarget::Implant(t) => by_type(t, &|k| k == ItemKind::Implant),
        DetailsTarget::Booster(t) => by_type(t, &|k| k == ItemKind::Booster),
    }
    .filter(|_| !fit.modules.is_empty() || !matches!(target, DetailsTarget::Module(..) | DetailsTarget::Charge(..)))
}

fn type_of_target(fit: &spai_fit::fit::Fit, target: DetailsTarget) -> Option<TypeId> {
    Some(match target {
        DetailsTarget::Type(t) | DetailsTarget::Drone(t) | DetailsTarget::Fighter(t) | DetailsTarget::Implant(t) | DetailsTarget::Booster(t) => t,
        DetailsTarget::Ship => fit.ship,
        DetailsTarget::Module(slot, index) => fit.module(slot, index)?.type_id,
        DetailsTarget::Charge(slot, index) => fit.module(slot, index)?.charge?,
    })
}

/// Who a modifier came from, as the tooltip names it.
fn who(calc: &Calc, idx: ItemIdx) -> String {
    let it = &calc.items()[idx];
    let name = calc.type_of(idx).map(|t| t.name.clone()).unwrap_or_else(|| format!("type {}", it.type_id));
    match it.kind {
        ItemKind::Skill => {
            let level = calc.base(idx, attr::SKILL_LEVEL) as u8;
            trf!("{skill} {level} (skill)", skill = name, level = ["0", "I", "II", "III", "IV", "V"].get(usize::from(level)).copied().unwrap_or("?"))
        }
        ItemKind::Ship => trf!("{name} (hull)", name = name),
        ItemKind::Char => tr!("Character").to_owned(),
        ItemKind::Charge => trf!("{name} (charge)", name = name),
        ItemKind::Implant => trf!("{name} (implant)", name = name),
        ItemKind::Booster => trf!("{name} (booster)", name = name),
        ItemKind::Projected { .. } => trf!("{name} (projected)", name = name),
        _ => name,
    }
}

fn what(step: &spai_fit::calc::Step) -> String {
    match step.op {
        Op::PreMul | Op::PostMul => format!("×{:.3}", step.value),
        Op::PreDiv | Op::PostDiv => format!("÷{:.3}", step.value),
        Op::PostPercent => format!("{:+.1}%", step.value),
        Op::ModAdd => format!("+{}", text::num(step.value)),
        Op::ModSub => format!("−{}", text::num(step.value)),
        Op::PreAssign | Op::PostAssign => format!("= {}", text::num(step.value)),
    }
}

/// Every attribute of a fitted item: the data's value, the fit's, and how it got there.
pub fn rows_for_item(dogma: &Dogma, calc: &Calc, item: ItemIdx, all: bool) -> Vec<AttrRow> {
    attrs_shown(dogma, &calc.attrs(item), all)
        .into_iter()
        .map(|a| {
            let def = dogma.attr(a);
            let unit = def.and_then(|d| d.unit);
            let high_is_good = def.is_none_or(|d| d.high_is_good);
            let e = calc.explain(item, a);
            let steps = e
                .steps
                .iter()
                .filter(|s| s.effect != 0.0 || matches!(s.op, Op::PreAssign | Op::PostAssign))
                .map(|s| StepRow {
                    who: match s.source {
                        Some(i) => who(calc, i),
                        None => tr!("Fleet boost or built-in rule").to_owned(),
                    },
                    what: what(s),
                    good: s.raises().map(|up| up == high_is_good),
                    kept: s.penalised.then_some(s.weight),
                })
                .collect();
            AttrRow {
                attr: a,
                label: label_of(dogma, a),
                raw: value_text(dogma, e.base, unit),
                fitted: Some(value_text(dogma, e.value, unit)),
                changed: (e.value - e.base).abs() > 1e-9 * e.base.abs().max(1.0),
                better: ((e.value - e.base).abs() > 1e-9 * e.base.abs().max(1.0)).then_some((e.value > e.base) == high_is_good),
                steps,
            }
        })
        .collect()
}

/// A type's attributes as the data has them.
pub fn rows_for_type(dogma: &Dogma, type_id: TypeId, all: bool) -> Vec<AttrRow> {
    let Some(t) = dogma.type_(type_id) else { return Vec::new() };
    let attrs: Vec<AttrId> = t.attrs.iter().map(|(a, _)| *a).collect();
    attrs_shown(dogma, &attrs, all)
        .into_iter()
        .map(|a| {
            let unit = dogma.attr(a).and_then(|d| d.unit);
            AttrRow { attr: a, label: label_of(dogma, a), raw: value_text(dogma, f64::from(t.attr(a).unwrap_or(0.0)), unit), fitted: None, changed: false, better: None, steps: Vec::new() }
        })
        .collect()
}

fn build(sim: &SimUi, target: DetailsTarget, all: bool) -> Option<Details> {
    let dogma = sim.dogma.as_ref()?;
    let (type_id, rows, fitted) = match target {
        DetailsTarget::Type(t) => (t, rows_for_type(dogma, t, all), false),
        _ => {
            let s = sim.session()?;
            let (resolved, env) = sim.outside().apply(dogma, &s.fit, sim.skills());
            let type_id = type_of_target(&resolved, target)?;
            let calc = Calc::new(dogma, &resolved, sim.skills(), &env);
            match item_of(&calc, &resolved, target) {
                Some(item) => (type_id, rows_for_item(dogma, &calc, item, all), true),
                None => (type_id, rows_for_type(dogma, type_id, all), false),
            }
        }
    };
    let t = dogma.type_(type_id)?;
    let group = dogma.group(t.group).map(|g| g.name.clone()).unwrap_or_default();
    let subtitle = match t.meta_level {
        Some(m) => trf!("{group} · Meta {m}", group = group, m = m),
        None => group,
    };
    Some(Details { title: t.name.clone(), subtitle, fitted, rows })
}

impl SimUi {
    /// Opens the details of an item, or brings its window to the front.
    pub fn open_details(&mut self, target: DetailsTarget) {
        if !self.details.iter().any(|w| w.target == target) {
            self.details.push(DetailsWindow::new(target));
        }
    }

    /// Whether the fit on screen has an item the target names, so the window can say when it
    /// shows the data's numbers only.
    fn details_key(&self) -> (u64, usize) {
        (self.session().map_or(0, |s| s.rev()), self.profile)
    }
}

/// Every open details window.
pub fn windows(sim: &mut SimUi, ctx: &egui::Context) {
    let key = sim.details_key();
    let mut closed: Vec<usize> = Vec::new();
    for i in 0..sim.details.len() {
        let (target, all) = (sim.details[i].target, sim.details[i].all);
        if sim.details[i].built_for != Some((key.0, key.1, all)) {
            let data = build(sim, target, all);
            let w = &mut sim.details[i];
            w.data = data;
            w.built_for = Some((key.0, key.1, all));
        }
        let Some(data) = sim.details[i].data.clone() else {
            closed.push(i);
            continue;
        };
        let mut open = true;
        let mut all = all;
        egui::Window::new(data.title.clone())
            .id(egui::Id::new(("sim_details", target)))
            .open(&mut open)
            .default_size([480.0, 440.0])
            .min_width(360.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&data.subtitle).weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut all, tr!("All attributes"));
                    });
                });
                if !data.fitted && !matches!(target, DetailsTarget::Type(_)) {
                    ui.label(egui::RichText::new(tr!("Not in the fit on screen: the data's numbers only.")).weak());
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    table(ui, &data);
                });
            });
        sim.details[i].all = all;
        if !open {
            closed.push(i);
        }
    }
    for i in closed.into_iter().rev() {
        sim.details.remove(i);
    }
}

fn table(ui: &mut egui::Ui, data: &Details) {
    let w = ui.available_width();
    let num_w = 110.0;
    let label_w = (w - if data.fitted { 2.0 * num_w } else { num_w } - 3.0 * crate::rows::GAP).max(120.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = crate::rows::GAP;
        crate::rows::head_cell(ui, label_w, tr!("Attribute"), false);
        crate::rows::head_cell(ui, num_w, if data.fitted { tr!("Base") } else { tr!("Value") }, true);
        if data.fitted {
            crate::rows::head_cell(ui, num_w, tr!("Fitted"), true);
        }
    });
    for row in &data.rows {
        crate::rows::row(ui, "details", ("sim_detail_row", row.attr), crate::rows::ROW_H - 2.0, false, |ui| {
            crate::rows::cell(ui, label_w, egui::RichText::new(&row.label), &row.label);
            crate::rows::num_cell(ui, num_w, row.raw.clone(), row.fitted.is_some() && row.changed);
            if let Some(f) = &row.fitted {
                let t = egui::RichText::new(f.clone()).monospace();
                let t = match row.better {
                    Some(true) => t.strong().color(GOOD),
                    Some(false) => t.strong().color(BAD),
                    None => t,
                };
                let r = ui.allocate_ui_with_layout(egui::vec2(num_w, crate::rows::ROW_H - 4.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_min_width(num_w);
                    ui.set_max_width(num_w);
                    ui.add(egui::Label::new(t).truncate().selectable(false))
                });
                if !row.steps.is_empty() {
                    r.response.on_hover_ui(|ui| breakdown(ui, row));
                }
            }
        });
    }
}

/// The math behind a fitted value: the base, every modifier with who did it, and the result.
fn breakdown(ui: &mut egui::Ui, row: &AttrRow) {
    ui.set_max_width(420.0);
    ui.label(egui::RichText::new(&row.label).strong());
    ui.label(trf!("Base {v}", v = row.raw));
    for s in &row.steps {
        let colour = match s.good {
            Some(true) => GOOD,
            Some(false) => BAD,
            None => ui.visuals().text_color(),
        };
        let mut line = format!("{}  {}", s.what, s.who);
        if let Some(k) = s.kept {
            line.push_str(&trf!("  (stacking: {pct}% kept)", pct = format!("{:.1}", k * 100.0)));
        }
        ui.label(egui::RichText::new(line).color(colour));
    }
    if let Some(f) = &row.fitted {
        ui.label(egui::RichText::new(trf!("= {v}", v = f)).strong());
    }
}

/// One side effect of a booster as the dialog shows it, its numbers after skills and implants.
#[derive(Clone, Debug, PartialEq)]
pub struct SideRow {
    pub effect: spai_fit::ids::EffectId,
    /// What it does: "Maximum Velocity −10.0%".
    pub label: String,
    /// The chance it rolls, `None` when the data has none.
    pub chance: Option<f64>,
    pub on: bool,
}

/// The side effects of a booster on the fit on screen.
pub fn side_rows(sim: &SimUi, type_id: TypeId) -> Vec<SideRow> {
    let (Some(dogma), Some(s)) = (sim.dogma.as_ref(), sim.session()) else { return Vec::new() };
    let Some(booster) = s.fit.boosters.iter().find(|b| b.type_id == type_id).cloned() else { return Vec::new() };
    let (fit, env) = sim.outside().apply(dogma, &s.fit, sim.skills());
    let calc = Calc::new(dogma, &fit, sim.skills(), &env);
    let item = calc.items().iter().position(|i| i.type_id == type_id && i.kind == ItemKind::Booster);
    let Some(t) = dogma.type_(type_id) else { return Vec::new() };
    t.effects
        .iter()
        .filter_map(|(e, _)| dogma.effect(*e).map(|d| (*e, d)))
        .filter(|(_, d)| d.usage_chance_attr.is_some())
        .map(|(e, d)| {
            let value = |a: AttrId| item.map_or_else(|| f64::from(t.attr(a).unwrap_or(0.0)), |i| calc.value(i, a));
            let parts: Vec<String> = d
                .modifiers
                .iter()
                .map(|m| {
                    let v = value(m.modifying);
                    let what = match m.op {
                        Op::PostPercent => format!("{v:+.1}%"),
                        _ => text::num(v),
                    };
                    format!("{} {what}", label_of(dogma, m.modified))
                })
                .collect();
            let label = if parts.is_empty() { d.name.clone() } else { parts.join(", ") };
            SideRow { effect: e, label, chance: d.usage_chance_attr.map(value), on: booster.side_effect_on(e) }
        })
        .collect()
}

impl SimUi {
    /// Takes or drops one side effect of a booster; the first pick turns the all-or-nothing switch
    /// into a choice per effect.
    pub fn set_side_effect(&mut self, type_id: TypeId, effect: Option<spai_fit::ids::EffectId>, on: bool, now_ms: u64) {
        let Some(dogma) = self.dogma.clone() else { return };
        let all: Vec<spai_fit::ids::EffectId> = dogma.type_(type_id).map(|t| t.effects.iter().filter(|(e, _)| dogma.effect(*e).is_some_and(|d| d.usage_chance_attr.is_some())).map(|(e, _)| *e).collect()).unwrap_or_default();
        self.edit(spai_fit::history::EditKind::Boosters, now_ms, move |f| {
            let Some(b) = f.boosters.iter_mut().find(|b| b.type_id == type_id) else { return };
            let mut list: Vec<spai_fit::ids::EffectId> = b.side_on.clone().unwrap_or_else(|| if b.side_effects { all.clone() } else { Vec::new() });
            match effect {
                Some(e) => {
                    list.retain(|x| *x != e);
                    if on {
                        list.push(e);
                    }
                }
                None => list = if on { all.clone() } else { Vec::new() },
            }
            b.side_effects = !list.is_empty();
            b.side_on = Some(list);
        });
    }
}

/// The side effects of the booster in `side_window`, each on or off with its chance.
pub fn side_effects_window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    let Some(type_id) = sim.side_window else { return };
    let rows = side_rows(sim, type_id);
    let name = sim.dogma.as_ref().and_then(|d| d.type_(type_id)).map_or(String::new(), |t| t.name.clone());
    let mut open = true;
    let mut set: Option<(Option<spai_fit::ids::EffectId>, bool)> = None;
    egui::Window::new(trf!("{name}: side effects", name = name)).id(egui::Id::new("sim_side_effects")).open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.label(egui::RichText::new(tr!("Each side effect rolls on its own when the booster is taken. Tick the ones to count; the figures are after your skills and implants.")).weak());
        ui.add_space(4.0);
        if rows.is_empty() {
            ui.label(egui::RichText::new(tr!("This booster has no side effects.")).weak());
        }
        egui::Grid::new("sim_side_grid").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
            for r in &rows {
                let mut on = r.on;
                if ui.checkbox(&mut on, egui::RichText::new(&r.label).color(BAD)).changed() {
                    set = Some((Some(r.effect), on));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(c) = r.chance {
                        ui.label(egui::RichText::new(trf!("{pct}% chance", pct = format!("{:.0}", c * 100.0))).weak());
                    }
                });
                ui.end_row();
            }
        });
        if !rows.is_empty() {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("All")).clicked() {
                    set = Some((None, true));
                }
                if ui.button(tr!("None")).clicked() {
                    set = Some((None, false));
                }
            });
        }
    });
    if let Some((e, on)) = set {
        sim.set_side_effect(type_id, e, on, now_ms);
    }
    if !open {
        sim.side_window = None;
    }
}
