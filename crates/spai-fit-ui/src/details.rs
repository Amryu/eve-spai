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
    /// Where the fitted column comes from when the item is not in the fit itself: the module a
    /// charge is loaded in, or the fit a drone is launched from.
    pub trial: Option<String>,
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
pub(crate) const GOOD: egui::Color32 = egui::Color32::from_rgb(0x7F, 0xC9, 0x8A);
pub(crate) const BAD: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x86, 0x7E);

/// How many attributes a tooltip shows.
const SUMMARY_ATTRS: usize = 6;

/// A type's pilot-facing attributes, those with a display name, or every one it carries.
pub(crate) fn attrs_shown(dogma: &Dogma, attrs: &[AttrId], all: bool) -> Vec<AttrId> {
    attrs.iter().copied().filter(|a| all || dogma.attr(*a).is_some_and(|d| !d.display.is_empty())).collect()
}

pub(crate) fn label_of(dogma: &Dogma, a: AttrId) -> String {
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
    type_tip_with(ui, dogma, type_id, note, None);
}

/// Every fitted item's attributes as the fit makes them, for the tooltips of the fit's rows.
pub type TipValues = std::collections::HashMap<DetailsTarget, std::collections::HashMap<AttrId, f64>>;

impl SimUi {
    /// The fitted values of every item of the fit on screen, figured once per change.
    pub fn tip_values(&mut self) -> std::sync::Arc<TipValues> {
        let key = (self.session().map_or(0, |s| s.rev()), self.profile);
        if let Some((k, v)) = &self.tip_cache {
            if *k == key {
                return v.clone();
            }
        }
        let mut out = TipValues::new();
        if let (Some(dogma), Some(s)) = (self.dogma.clone(), self.session()) {
            let (fit, env) = self.outside().apply(&dogma, &s.fit, self.skills());
            let calc = Calc::new(&dogma, &fit, self.skills(), &env);
            for (i, it) in calc.items().iter().enumerate() {
                let target = match it.kind {
                    ItemKind::Ship => DetailsTarget::Ship,
                    ItemKind::Module { slot, index } => DetailsTarget::Module(slot, index),
                    ItemKind::Drone { .. } => DetailsTarget::Drone(it.type_id),
                    ItemKind::Fighter { .. } => DetailsTarget::Fighter(it.type_id),
                    ItemKind::Implant => DetailsTarget::Implant(it.type_id),
                    ItemKind::Booster => DetailsTarget::Booster(it.type_id),
                    _ => continue,
                };
                out.entry(target).or_insert_with(|| calc.attrs(i).into_iter().map(|a| (a, calc.value(i, a))).collect());
            }
        }
        let out = std::sync::Arc::new(out);
        self.tip_cache = Some((key, out.clone()));
        out
    }
}

/// The tooltip with the fitted values in place of the data's, where there are any; a value the
/// fit improved is tinted green, one it worsened red.
pub fn type_tip_with(ui: &mut egui::Ui, dogma: &Dogma, type_id: TypeId, note: &str, fitted: Option<&std::collections::HashMap<AttrId, f64>>) {
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
    let line = if fitted.is_some() { if line.is_empty() { tr!("As fitted").to_owned() } else { trf!("{line} · as fitted", line = line) } } else { line };
    if !line.is_empty() {
        ui.label(egui::RichText::new(line).weak());
    }
    let attrs: Vec<AttrId> = t.attrs.iter().map(|(a, _)| *a).collect();
    let shown: Vec<AttrId> = attrs_shown(dogma, &attrs, false).into_iter().take(SUMMARY_ATTRS).collect();
    if !shown.is_empty() {
        ui.add_space(2.0);
        egui::Grid::new(("sim_tip", type_id)).num_columns(2).spacing([16.0, 1.0]).show(ui, |ui| {
            for a in shown {
                let base = f64::from(t.attr(a).unwrap_or(0.0));
                let v = fitted.and_then(|f| f.get(&a)).copied().unwrap_or(base);
                let def = dogma.attr(a);
                let text = egui::RichText::new(value_text(dogma, v, def.and_then(|d| d.unit))).monospace();
                let text = if (v - base).abs() > 1e-9 * base.abs().max(1.0) {
                    if (v > base) == def.is_none_or(|d| d.high_is_good) { text.color(GOOD) } else { text.color(BAD) }
                } else {
                    text
                };
                ui.label(egui::RichText::new(label_of(dogma, a)).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(text);
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
    // Skills reach attributes on every ship (jump drives, conduit passengers) that the hull itself
    // has none of: only what the item carries, and not what reads nothing.
    let carried: Vec<AttrId> = dogma.type_(calc.items()[item].type_id).map(|t| t.attrs.iter().map(|(a, _)| *a).collect()).unwrap_or_default();
    attrs_shown(dogma, &calc.attrs(item), all)
        .into_iter()
        .filter_map(|a| {
            let e = calc.explain(item, a);
            (all || (carried.contains(&a) && !(e.value == 0.0 && e.base == 0.0))).then_some((a, e))
        })
        .map(|(a, e)| {
            let def = dogma.attr(a);
            let unit = def.and_then(|d| d.unit);
            let high_is_good = def.is_none_or(|d| d.high_is_good);
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
        .filter(|a| all || t.attr(*a).is_some_and(|v| v != 0.0))
        .map(|a| {
            let unit = dogma.attr(a).and_then(|d| d.unit);
            AttrRow { attr: a, label: label_of(dogma, a), raw: value_text(dogma, f64::from(t.attr(a).unwrap_or(0.0)), unit), fitted: None, changed: false, better: None, steps: Vec::new() }
        })
        .collect()
}

/// The charge's total damage as the calculation makes it, times the module's multiplier.
fn charge_score(calc: &Calc, module: ItemIdx) -> f64 {
    let Some(c) = calc.items()[module].charge else { return 0.0 };
    let damage: f64 = [attr::EM_DAMAGE, attr::THERMAL_DAMAGE, attr::KINETIC_DAMAGE, attr::EXPLOSIVE_DAMAGE].iter().map(|a| calc.value(c, *a)).sum();
    let mult = if calc.has(module, attr::DAMAGE_MULTIPLIER) { calc.value(module, attr::DAMAGE_MULTIPLIER) } else { 1.0 };
    damage * mult
}

/// A charge or drone not in the fit, put into a copy of it: the charge in the module of the fit
/// that does the most with it, else in a Tech II module that takes it (Tech I without one); the
/// drone launched. The copy, the target in it and what was done.
pub fn trial(dogma: &Dogma, fit: &spai_fit::fit::Fit, calc_of: &dyn Fn(&spai_fit::fit::Fit, Slot, u8) -> f64, type_id: TypeId) -> Option<(spai_fit::fit::Fit, DetailsTarget, String)> {
    use spai_fit::ids::category;
    let t = dogma.type_(type_id)?;
    if t.category == category::DRONE {
        let mut f = fit.clone();
        f.drones.retain(|d| d.type_id != type_id);
        f.drones.push(spai_fit::fit::DroneStack { type_id, count: 1, active: true, launched: None, mutation: None });
        return Some((f, DetailsTarget::Drone(type_id), tr!("Launched from the fit on screen").to_owned()));
    }
    if t.category != category::CHARGE {
        return None;
    }
    let loaded = |m: &spai_fit::fit::Module| {
        let mut f = fit.clone();
        let slot = f.modules.iter_mut().find(|x| x.slot == m.slot && x.index == m.index)?;
        slot.charge = Some(type_id);
        Some(f)
    };
    // One of each kind of module: a rack of the same guns gives the same numbers.
    let mut seen = Vec::new();
    let fitted = fit.modules.iter().filter(|m| dogma.takes_charge(m.type_id, type_id) && !seen.contains(&m.type_id) && {
        seen.push(m.type_id);
        true
    });
    let best_fitted = fitted
        .filter_map(|m| loaded(m).map(|f| (calc_of(&f, m.slot, m.index), m, f)))
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then((b.1.slot, b.1.index).cmp(&(a.1.slot, a.1.index))));
    if let Some((_, m, f)) = best_fitted {
        let name = spai_fit::formats::name_of(dogma, m.type_id);
        return Some((f, DetailsTarget::Charge(m.slot, m.index), trf!("Loaded in {module} on the fit on screen", module = name)));
    }
    // None fitted: a Tech II module that takes it, else Tech I, else any.
    let tier = |m: &spai_fit::dogma::TypeDef| match m.meta_group {
        Some(2) => 2,
        Some(1) | None if m.tech.unwrap_or(1) == 1 => 1,
        _ => 0,
    };
    // The plain one of its tier (Torpedo Launcher II over a Polarized one), then the most damage,
    // then the smallest (Heavy Capacitor Booster II over the capital one).
    let plain = |m: &spai_fit::dogma::TypeDef| m.name.ends_with(if tier(m) == 2 { " II" } else { " I" });
    let rank = |m: &spai_fit::dogma::TypeDef| (tier(m), plain(m), m.attr(attr::DAMAGE_MULTIPLIER).unwrap_or(1.0), -m.attr(attr::POWER).unwrap_or(0.0));
    let module = dogma
        .types
        .iter()
        .filter(|m| m.published && m.category == category::MODULE && dogma.takes_charge(m.id, type_id))
        .max_by(|a, b| rank(a).partial_cmp(&rank(b)).unwrap_or(std::cmp::Ordering::Equal).then(b.name.cmp(&a.name)))?;
    let slot = dogma.slot_of(module.id)?;
    let mut f = fit.clone();
    let index = f.modules.iter().filter(|m| m.slot == slot).map(|m| m.index + 1).max().unwrap_or(0);
    f.modules.push(spai_fit::fit::Module::new(module.id, slot, index).with_charge(type_id));
    Some((f, DetailsTarget::Charge(slot, index), trf!("As if loaded in {module}, which the fit on screen does not carry", module = module.name)))
}

pub(crate) fn build(sim: &SimUi, target: DetailsTarget, all: bool) -> Option<Details> {
    let dogma = sim.dogma.as_ref()?;
    let mut trial_note = None;
    let (type_id, rows, fitted) = match target {
        DetailsTarget::Type(t) if !trial_kind(dogma, t) || sim.session().is_none() => (t, rows_for_type(dogma, t, all), false),
        _ => {
            let s = sim.session()?;
            let (resolved, env) = sim.outside().apply(dogma, &s.fit, sim.skills());
            let type_id = match target {
                DetailsTarget::Type(t) => t,
                _ => type_of_target(&resolved, target)?,
            };
            let calc = Calc::new(dogma, &resolved, sim.skills(), &env);
            match item_of(&calc, &resolved, target) {
                Some(item) => (type_id, rows_for_item(dogma, &calc, item, all), true),
                None => {
                    let score = |f: &spai_fit::fit::Fit, slot: Slot, index: u8| {
                        let c = Calc::new(dogma, f, sim.skills(), &env);
                        c.module(slot, index).map_or(0.0, |m| charge_score(&c, m))
                    };
                    let tried = trial(dogma, &resolved, &score, type_id).and_then(|(f, at, note)| {
                        let c = Calc::new(dogma, &f, sim.skills(), &env);
                        let item = item_of(&c, &f, at)?;
                        Some((rows_for_item(dogma, &c, item, all), note))
                    });
                    match tried {
                        Some((rows, note)) => {
                            trial_note = Some(note);
                            (type_id, rows, true)
                        }
                        None => (type_id, rows_for_type(dogma, type_id, all), false),
                    }
                }
            }
        }
    };
    let t = dogma.type_(type_id)?;
    let group = dogma.group(t.group).map(|g| g.name.clone()).unwrap_or_default();
    let subtitle = match t.meta_level {
        Some(m) => trf!("{group} · Meta {m}", group = group, m = m),
        None => group,
    };
    Some(Details { title: t.name.clone(), subtitle, fitted, trial: trial_note, rows })
}

/// A charge or a drone, which the window shows as the fit on screen would use it.
fn trial_kind(dogma: &Dogma, type_id: TypeId) -> bool {
    dogma.type_(type_id).is_some_and(|t| t.category == spai_fit::ids::category::CHARGE || t.category == spai_fit::ids::category::DRONE)
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
                if let Some(note) = &data.trial {
                    ui.add(egui::Label::new(format!("{} {note}", egui_phosphor::regular::INFO)).wrap());
                } else if !data.fitted && !matches!(target, DetailsTarget::Type(_)) {
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
            crate::rows::head_cell(ui, num_w, if data.trial.is_some() { tr!("Modified") } else { tr!("Fitted") }, true);
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
    let (fit, env) = sim.outside().apply(dogma, &s.fit, sim.skills());
    let Some(booster) = fit.boosters.iter().find(|b| b.type_id == type_id).cloned() else { return Vec::new() };
    let calc = Calc::new(dogma, &fit, sim.skills(), &env);
    let item = calc.items().iter().position(|i| i.type_id == type_id && i.kind == ItemKind::Booster);
    let Some(t) = dogma.type_(type_id) else { return Vec::new() };
    t.effects
        .iter()
        .filter_map(|(e, _)| dogma.effect(*e).map(|d| (*e, d)))
        .filter(|(_, d)| d.usage_chance_attr.is_some())
        .map(|(e, d)| {
            let value = |a: AttrId| item.map_or_else(|| f64::from(t.attr(a).unwrap_or(0.0)), |i| calc.value(i, a));
            // One line per attribute and change, saying what it reaches: the same attribute is the
            // ship's speed on one side effect and missile velocity on another, and one side effect
            // can reach it twice through two skills.
            let mut parts: Vec<(String, Vec<String>)> = Vec::new();
            for m in &d.modifiers {
                let v = value(m.modifying);
                let what = match m.op {
                    Op::PostPercent => format!("{v:+.1}%"),
                    _ => text::num(v),
                };
                let head = format!("{} {what}", label_of(dogma, m.modified));
                let pilot = m.domain == spai_fit::dogma::Domain::Char && m.skill.is_none();
                let scope = match m.skill {
                    Some(spai_fit::dogma::SkillRef::Of(sk)) => Some(spai_fit::formats::name_of(dogma, sk)),
                    _ => None,
                };
                let head = if pilot { trf!("{what} on the pilot", what = head) } else { head };
                match parts.iter_mut().find(|(h, _)| *h == head) {
                    Some((_, scopes)) => {
                        if let Some(sc) = scope {
                            if !scopes.contains(&sc) {
                                scopes.push(sc);
                            }
                        }
                    }
                    None => parts.push((head, scope.into_iter().collect())),
                }
            }
            let lines: Vec<String> = parts
                .into_iter()
                .map(|(head, scopes)| if head.ends_with(tr!("on the pilot")) { head } else if scopes.is_empty() { trf!("{what} on the ship", what = head) } else { trf!("{what} for whatever needs {skills}", what = head, skills = scopes.join(tr!(" or "))) })
                .collect();
            let label = if lines.is_empty() { d.name.clone() } else { lines.join("; ") };
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
