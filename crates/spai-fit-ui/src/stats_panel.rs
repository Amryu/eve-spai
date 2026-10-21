//! What the fit computes to: sections of labelled numbers, tight.

use spai_fit::stats::{Computed, WeaponKind};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::rows;
use crate::{text, SimUi};

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let tree = sim.browser.tree(&dogma);
    let Some(computed) = sim.computed() else {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("No fit open.")).weak());
        return;
    };
    egui::ScrollArea::vertical().id_salt("sim_stats_scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.add_space(2.0);
        fitting(ui, &computed);
        let profile = sim.session().map(|s| s.fit.damage_profile).unwrap_or_default();
        if let Some(p) = defense(ui, &dogma, &tree, &computed, profile) {
            sim.set_damage_profile(p);
        }
        offense(ui, &dogma, &computed);
        capacitor(ui, &computed);
        navigation(ui, &computed);
        targeting(ui, &computed);
        drones(ui, &computed);
        remote(ui, &computed);
        cost(ui, sim.cost());
    });
}

/// A section title a step larger than the lines under it, with a rule beneath.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    let size = egui::TextStyle::Body.resolve(ui.style()).size + 2.0;
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), size + 6.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(rows::GAP);
        ui.add(egui::Label::new(egui::RichText::new(title).strong().size(size)).selectable(false));
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 2.0 * rows::GAP, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect.translate(egui::vec2(rows::GAP, 0.0)), 0.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    ui.add_space(3.0);
}

/// A labelled value on one line of fixed height, the value right-aligned.
pub fn line(ui: &mut egui::Ui, label: &str, value: String) {
    line_colored(ui, label, value, None);
}

fn line_colored(ui: &mut egui::Ui, label: &str, value: String, color: Option<egui::Color32>) {
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 19.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(rows::GAP);
        ui.add(egui::Label::new(label).selectable(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(rows::GAP);
            let t = egui::RichText::new(value).monospace();
            let t = match color {
                Some(c) => t.color(c),
                None => t,
            };
            ui.add(egui::Label::new(t).truncate().selectable(false));
        });
    });
}

/// A used-of-total line with a thin bar under it in the resource's colour, red past the total.
/// Used and total as the bar prints them: two decimals once the two are within 1% or one unit of
/// each other, where rounding could hide whether the fit still fits.
pub(crate) fn resource_text(used: f64, total: f64, close_call: bool) -> (String, String) {
    if close_call && (used - total).abs() <= (total.abs() * 0.01).max(1.0) {
        (format!("{used:.2}"), format!("{total:.2}"))
    } else {
        (text::num(used), text::num(total))
    }
}

fn bar(ui: &mut egui::Ui, label: &str, used: f64, total: f64, unit: &str, color: egui::Color32, close_call: bool) {
    let over = used > total + 1e-6;
    let (u, t) = resource_text(used, total, close_call);
    let txt = trf!("{used} / {total} {unit}", used = u, total = t, unit = unit);
    line_colored(ui, label, txt, over.then(|| ui.visuals().error_fg_color));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 2.0 * rows::GAP, 3.0), egui::Sense::hover());
    let rect = rect.translate(egui::vec2(rows::GAP, 0.0));
    let fill = if total > 0.0 { (used / total).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let painter = ui.painter();
    painter.rect_filled(rect, 1.5, ui.visuals().faint_bg_color);
    let color = if over { ui.visuals().error_fg_color } else { color };
    painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fill, rect.height())), 1.5, color);
    ui.add_space(2.0);
}

fn fitting(ui: &mut egui::Ui, c: &Computed) {
    let f = &c.fitting;
    section(ui, tr!("Fitting"));
    bar(ui, tr!("CPU"), f.cpu_used, f.cpu_total, "tf", spai_ui::colors::CPU, true);
    bar(ui, tr!("Powergrid"), f.power_used, f.power_total, "MW", spai_ui::colors::POWERGRID, true);
    if f.calibration_total > 0.0 {
        bar(ui, tr!("Calibration"), f.calibration_used, f.calibration_total, "", spai_ui::colors::CALIBRATION, false);
    }
    line(ui, tr!("Turrets / launchers"), trf!("{t1}/{t2}  {l1}/{l2}", t1 = f.turrets_used, t2 = f.turrets_total, l1 = f.launchers_used, l2 = f.launchers_total));
}

/// A local repair as hit points per second while it runs (an ancillary booster on its charges, a
/// repairer on paste), and, when reloading lowers it, the average with reloads in brackets.
fn rep_text(sustained: f64, running: f64) -> String {
    if (running - sustained).abs() > 1e-6 * sustained.abs().max(1.0) {
        trf!("{burst}/s ({sustained}/s with reloads)", burst = text::num(running), sustained = text::num(sustained))
    } else {
        trf!("{v}/s", v = text::num(running))
    }
}

/// A resistance as the game shows one: its damage colour filled to the share resisted over a
/// darker remainder, the figure on top.
fn resist_cell(ui: &mut egui::Ui, width: f32, resist: f64, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 18.0), egui::Sense::hover());
    let bar = rect.shrink2(egui::vec2(2.0, 1.0));
    let painter = ui.painter();
    painter.rect_filled(bar, 2.0, color.gamma_multiply(0.22));
    let f = resist.clamp(0.0, 1.0) as f32;
    painter.rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * f, bar.height())), 2.0, color.gamma_multiply(0.75));
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    painter.text(bar.center(), egui::Align2::CENTER_CENTER, format!("{:.0}%", resist * 100.0), font, egui::Color32::WHITE);
}

/// One entry of the incoming damage menu: a name and the type whose split it stands for.
pub(crate) struct ProfileEntry {
    pub label: String,
    pub type_id: spai_fit::ids::TypeId,
}

/// The incoming damage menu's families, each a flat list: the market's families (Hybrid Charges,
/// Missiles, Combat Drones), and in each the damaging items folded together where they deal the
/// same thing. A drone line is one entry from its Tech I to its faction copies; charges dealing the
/// same split in one family are one entry whatever their size, named by what their names share
/// ("Scourge", "EMP", "Antimatter Charge").
pub(crate) fn profile_families(dogma: &Dogma, tree: &crate::browser::Tree) -> Vec<(String, Vec<ProfileEntry>)> {
    use std::collections::BTreeMap;
    fn walk(tree: &crate::browser::Tree, g: spai_fit::ids::MarketGroupId, out: &mut Vec<spai_fit::ids::TypeId>) {
        out.extend(tree.types(g).iter().copied());
        for &c in tree.children(g) {
            walk(tree, c, out);
        }
    }
    let mut families = Vec::new();
    for &root in tree.roots() {
        for &family in tree.children(root) {
            let mut types = Vec::new();
            walk(tree, family, &mut types);
            // Class key: a drone or fighter line by its Tech I; a charge by its split.
            let mut classes: BTreeMap<String, Vec<spai_fit::ids::TypeId>> = BTreeMap::new();
            for t in types {
                let Some(def) = dogma.type_(t) else { continue };
                // What is fired or launched: charges, drones, fighters; not the weapons themselves.
                if !matches!(def.category, spai_fit::ids::category::CHARGE | spai_fit::ids::category::DRONE | spai_fit::ids::category::FIGHTER) {
                    continue;
                }
                let Some(p) = profile_of(dogma, &def.name) else { continue };
                let key = if matches!(def.category, spai_fit::ids::category::DRONE | spai_fit::ids::category::FIGHTER) {
                    format!("line {}", def.variation_parent.unwrap_or(t))
                } else {
                    let w = p.weights();
                    // Whole percent: the sizes of one charge differ in the last decimals.
                    format!("split {} {} {} {}", (w[0] * 100.0).round(), (w[1] * 100.0).round(), (w[2] * 100.0).round(), (w[3] * 100.0).round())
                };
                classes.entry(key).or_default().push(t);
            }
            let mut entries: Vec<ProfileEntry> = classes.into_values().filter_map(|members| class_entry(dogma, &members)).collect();
            if entries.is_empty() {
                continue;
            }
            entries.sort_by(|a, b| a.label.cmp(&b.label));
            // One split can still come from two names a family keeps apart; the first stays.
            entries.dedup_by(|a, b| a.label == b.label);
            families.push((tree.name(family).to_owned(), entries));
        }
    }
    families
}

/// A class's entry: named by the words its plain (Tech I and II) members' names share, the
/// plainest member standing for it.
fn class_entry(dogma: &Dogma, members: &[spai_fit::ids::TypeId]) -> Option<ProfileEntry> {
    let defs: Vec<&spai_fit::dogma::TypeDef> = members.iter().filter_map(|t| dogma.type_(*t)).collect();
    let plain: Vec<&&spai_fit::dogma::TypeDef> = defs.iter().filter(|d| d.meta_group.is_none_or(|m| m <= 2)).collect();
    let named: Vec<&spai_fit::dogma::TypeDef> = if plain.is_empty() { defs.clone() } else { plain.into_iter().copied().collect() };
    let rep = named.iter().min_by_key(|d| (d.meta_level.unwrap_or(0), d.name.len()))?;
    let words: Vec<Vec<&str>> = named.iter().map(|d| d.name.split_whitespace().collect()).collect();
    let mut shared: Vec<&str> = Vec::new();
    for (i, w) in words[0].iter().enumerate() {
        if words.iter().all(|ws| ws.get(i) == Some(w)) {
            shared.push(w);
        } else {
            break;
        }
    }
    // A lone member or one whose name ends in a size or a tech mark: drop those.
    while shared.last().is_some_and(|w| matches!(*w, "S" | "M" | "L" | "XL" | "I" | "II")) {
        shared.pop();
    }
    let mut label = if shared.is_empty() { rep.name.clone() } else { shared.join(" ") };
    for size in [" XL", " S", " M", " L"] {
        if let Some(l) = label.strip_suffix(size) {
            label = l.to_owned();
            break;
        }
    }
    Some(ProfileEntry { label, type_id: rep.id })
}

/// A charge's damage split as a profile.
fn profile_of(dogma: &Dogma, name: &str) -> Option<spai_fit::fit::DamageProfile> {
    use spai_fit::dogma::attr;
    let t = dogma.type_named(name)?;
    let d = |a| t.attr(a).unwrap_or(0.0);
    let p = spai_fit::fit::DamageProfile { em: d(attr::EM_DAMAGE), thermal: d(attr::THERMAL_DAMAGE), kinetic: d(attr::KINETIC_DAMAGE), explosive: d(attr::EXPLOSIVE_DAMAGE) };
    (p.em + p.thermal + p.kinetic + p.explosive > 0.0).then_some(p)
}

/// The layers, their resists, and the damage expected in, which the effective HP is figured
/// against; a right click on it picks another. Returns the profile picked.
fn defense(ui: &mut egui::Ui, dogma: &Dogma, tree: &crate::browser::Tree, c: &Computed, profile: spai_fit::fit::DamageProfile) -> Option<spai_fit::fit::DamageProfile> {
    let mut picked = None;
    let t = &c.tank;
    section(ui, tr!("Defense"));
    line(ui, tr!("Effective HP"), text::num(t.ehp));
    if t.invulnerable {
        ui.add(egui::Label::new(egui::RichText::new(tr!("The PANIC core runs: near total shield resistances, worked out from an in-game figure since the game data gives none.")).weak()).wrap());
    }
    let w = ui.available_width() - 2.0 * rows::GAP;
    let (name_w, hp_w) = (52.0, 54.0);
    let res_w = ((w - name_w - hp_w) / 4.0).max(30.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(rows::GAP);
        rows::head_cell(ui, name_w, "", false);
        rows::head_cell(ui, hp_w, "HP", true);
        for (i, h) in ["EM", "TH", "KIN", "EXP"].into_iter().enumerate() {
            rows::head_cell_centered(ui, res_w, h, spai_ui::colors::DAMAGE[i]);
        }
    });
    for (name, layer) in [(tr!("Shield"), &t.shield), (tr!("Armor"), &t.armor), (tr!("Hull"), &t.hull)] {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(rows::GAP);
            ui.add_sized([name_w, 18.0], egui::Label::new(name).halign(egui::Align::LEFT).selectable(false));
            rows::num_cell(ui, hp_w, text::num(layer.hp), false);
            for i in 0..4 {
                resist_cell(ui, res_w, layer.resist(i), spai_ui::colors::DAMAGE[i]);
            }
        });
    }
    let weights = profile.weights();
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(rows::GAP);
        // The caret says the row opens a menu, as a dropdown would.
        ui.add_sized([name_w + hp_w, 18.0], egui::Label::new(egui::RichText::new(format!("{} {}", tr!("Incoming"), egui_phosphor::regular::CARET_DOWN))).halign(egui::Align::LEFT).selectable(false));
        for (i, w) in weights.iter().enumerate() {
            rows::num_cell_centered(ui, res_w, format!("{:.0}%", w * 100.0), spai_ui::colors::DAMAGE[i]);
        }
    });
    let r = ui.interact(row.response.rect, egui::Id::new("sim_damage_profile"), egui::Sense::click());
    if r.hovered() {
        ui.painter().rect_filled(row.response.rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.6));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let r = r.on_hover_text(tr!("The damage expected in, which the effective HP is figured against. Click to change it."));
    // Either click opens it at the pointer; it stays open while the custom fields are edited, and a
    // pick or a click elsewhere closes it.
    let toggle = (r.clicked() || r.secondary_clicked()).then_some(egui::SetOpenCommand::Toggle);
    egui::Popup::menu(&r).open_memory(toggle).at_pointer_fixed().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(220.0);
        if ui.button(tr!("Uniform")).clicked() {
            picked = Some(spai_fit::fit::DamageProfile::UNIFORM);
            ui.close();
        }
        // Every damaging charge, drone and fighter the market sells, one level per family.
        let families = ui.ctx().memory_mut(|m| m.data.get_temp_mut_or_insert_with(egui::Id::new("sim_profile_families"), || std::sync::Arc::new(profile_families(dogma, tree))).clone());
        for (family, entries) in families.iter() {
            ui.menu_button(family.as_str(), |ui| {
                ui.set_min_width(220.0);
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for e in entries {
                        let Some(p) = dogma.type_(e.type_id).and_then(|t| profile_of(dogma, &t.name)) else { continue };
                        let item = egui::Button::image_and_text(egui::Image::new(crate::icons::type_icon_uri(e.type_id)).fit_to_exact_size(egui::vec2(16.0, 16.0)), e.label.as_str()).frame_when_inactive(false);
                        if ui.add(item).clicked() {
                            picked = Some(p);
                            ui.close();
                        }
                    }
                });
            });
        }
        ui.separator();
        ui.label(egui::RichText::new(tr!("Custom")).weak());
        let mut custom = profile;
        let mut changed = false;
        let mut only: Option<usize> = None;
        egui::Grid::new("sim_profile_custom").num_columns(3).show(ui, |ui| {
            for (i, (label, v)) in [("EM", &mut custom.em), ("TH", &mut custom.thermal), ("KIN", &mut custom.kinetic), ("EXP", &mut custom.explosive)].into_iter().enumerate() {
                ui.label(egui::RichText::new(label).color(spai_ui::colors::DAMAGE[i]));
                changed |= ui.add(egui::DragValue::new(v).range(0.0..=1000.0).speed(0.5)).changed();
                if ui.small_button("100%").on_hover_text(trf!("Only {kind} damage", kind = label)).clicked() {
                    only = Some(i);
                }
                ui.end_row();
            }
        });
        if changed {
            picked = Some(custom);
        }
        if let Some(i) = only {
            let mut p = spai_fit::fit::DamageProfile { em: 0.0, thermal: 0.0, kinetic: 0.0, explosive: 0.0 };
            *[&mut p.em, &mut p.thermal, &mut p.kinetic, &mut p.explosive][i] = 1.0;
            picked = Some(p);
            ui.close();
        }
    });
    if t.reps.shield > 0.0 || t.reps.shield_burst > 0.0 {
        line(ui, tr!("Shield boost"), rep_text(t.reps.shield, t.reps.shield_burst));
    }
    if t.reps.armor > 0.0 || t.reps.armor_burst > 0.0 {
        line(ui, tr!("Armor repair"), rep_text(t.reps.armor, t.reps.armor_burst));
    }
    if t.reps.hull > 0.0 {
        line(ui, tr!("Hull repair"), trf!("{v}/s", v = text::num(t.reps.hull)));
    }
    line(ui, tr!("Shield regen (peak)"), trf!("{v}/s", v = text::num(t.shield_peak_regen)));
    picked
}

fn offense(ui: &mut egui::Ui, dogma: &Dogma, c: &Computed) {
    section(ui, tr!("Offense"));
    // A total only where there is more than one weapon to add up; one alone shows its own below.
    if c.weapons.len() > 1 {
        line(ui, tr!("Total DPS / volley"), trf!("{dps} / {volley}", dps = text::num(c.dps()), volley = text::num(c.volley())));
    }
    for w in &c.weapons {
        let name = spai_fit::formats::name_of(dogma, w.type_id);
        let charge = w.charge.map(|c| spai_fit::formats::name_of(dogma, c));
        let title = match &charge {
            Some(ch) => trf!("{n} x {name} · {charge}", n = w.count, name = name, charge = ch),
            None => trf!("{n} x {name}", n = w.count, name = name),
        };
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 19.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(rows::GAP);
            ui.add(egui::Label::new(egui::RichText::new(&title).weak()).truncate()).on_hover_text(&title);
        });
        line(ui, tr!("DPS / volley"), trf!("{dps} / {volley}", dps = text::num(w.dps), volley = text::num(w.volley.total())));
        match w.kind {
            WeaponKind::Missile | WeaponKind::Fighter => {
                if let Some(m) = &w.missile {
                    line(ui, tr!("Range"), text::metres(m.range));
                    line(ui, tr!("Explosion"), trf!("{r} m, {v} m/s", r = text::num(m.explosion_radius), v = text::num(m.explosion_velocity)));
                }
            }
            WeaponKind::Smartbomb => line(ui, tr!("Range"), text::metres(w.optimal)),
            _ => {
                line(ui, tr!("Optimal + falloff"), trf!("{o} + {f}", o = text::metres(w.optimal), f = text::metres(w.falloff)));
                line(ui, tr!("Tracking"), text::num(w.tracking));
            }
        }
        if w.spool_max > 0.0 {
            // Spooled as set, the most it reaches, and how long it takes from cold.
            line(ui, tr!("Spool"), trf!("+{now}% of +{max}%", now = text::num(w.spool_max * w.spool_frac * 100.0), max = text::num(w.spool_max * 100.0)));
            if let Some(n) = w.spool_cycles() {
                line(ui, tr!("Full after"), trf!("{n} cycles, {s}", n = n, s = text::secs(f64::from(n) * w.cycle_s)));
            }
            line(ui, tr!("DPS cold / full"), format!("{} / {}", text::num(w.cold_dps()), text::num(w.cold_dps() * (1.0 + w.spool_max))));
        }
    }
}

fn capacitor(ui: &mut egui::Ui, c: &Computed) {
    let cap = &c.cap;
    section(ui, tr!("Capacitor"));
    line(ui, tr!("Capacity"), trf!("{v} GJ", v = text::num(cap.capacity)));
    line(ui, tr!("Recharge"), text::secs(cap.recharge_s));
    line(ui, tr!("Peak regen / usage"), trf!("{p} / {u} GJ/s", p = text::num(cap.peak_regen), u = text::num(cap.usage)));
    match (cap.stable_at, cap.lasts_s) {
        (Some(p), _) => line(ui, tr!("Stable at"), text::pct(p)),
        (None, Some(t)) => line_colored(ui, tr!("Lasts"), text::secs(t), Some(ui.visuals().warn_fg_color)),
        _ => {}
    }
}

fn navigation(ui: &mut egui::Ui, c: &Computed) {
    let m = &c.mobility;
    section(ui, tr!("Navigation"));
    line(ui, tr!("Max velocity"), trf!("{v} m/s", v = text::num(m.max_velocity)));
    line(ui, tr!("Align time"), text::secs(m.align_s));
    line(ui, tr!("Warp speed"), trf!("{v} AU/s", v = text::num(m.warp_speed_au)));
    line(ui, tr!("Signature"), trf!("{v} m", v = text::num(m.signature)));
    line(ui, tr!("Mass"), trf!("{v} kg", v = text::num(m.mass)));
}

fn targeting(ui: &mut egui::Ui, c: &Computed) {
    let s = &c.sensors;
    section(ui, tr!("Targeting"));
    line(ui, tr!("Lock range"), text::metres(s.lock_range));
    line(ui, tr!("Scan resolution"), trf!("{v} mm", v = text::num(s.scan_resolution)));
    line(ui, tr!("Max targets"), s.max_targets.to_string());
    let kind = match s.kind {
        spai_fit::stats::SensorKind::Radar => tr!("Radar"),
        spai_fit::stats::SensorKind::Ladar => tr!("Ladar"),
        spai_fit::stats::SensorKind::Magnetometric => tr!("Magnetometric"),
        spai_fit::stats::SensorKind::Gravimetric => tr!("Gravimetric"),
    };
    line(ui, tr!("Sensor strength"), trf!("{v} {kind}", v = text::num(s.strength), kind = kind));
}

fn drones(ui: &mut egui::Ui, c: &Computed) {
    let d = &c.drones;
    if d.bay_total <= 0.0 && d.active == 0 {
        return;
    }
    section(ui, tr!("Drones"));
    let c = ui.visuals().hyperlink_color;
    bar(ui, tr!("Bandwidth"), d.bandwidth_used, d.bandwidth_total, "Mbit/s", c, false);
    bar(ui, tr!("Drone bay"), d.bay_used, d.bay_total, "m³", c, false);
    line(ui, tr!("In space"), trf!("{n} of {max}", n = d.active, max = d.max_active));
    line(ui, tr!("Control range"), text::metres(d.control_range));
}

/// What the fit costs at the market's average prices: the ship as it flies, and by part.
fn cost(ui: &mut egui::Ui, c: Option<crate::Cost>) {
    let Some(c) = c else { return };
    section(ui, tr!("Cost"));
    if c.total() <= 0.0 && c.missing > 0 {
        line(ui, tr!("Prices"), tr!("loading…").to_owned());
        return;
    }
    let isk = |v: f64| trf!("{v} ISK", v = text::num(v));
    line(ui, tr!("Ship as fitted"), isk(c.fitted()));
    for (label, v) in [(tr!("Hull"), c.hull), (tr!("Modules"), c.modules), (tr!("Charges loaded"), c.charges), (tr!("Drones and fighters"), c.drones), (tr!("Implants and boosters"), c.worn), (tr!("Cargo"), c.cargo)] {
        if v > 0.0 {
            line(ui, label, isk(v));
        }
    }
    line(ui, tr!("Everything"), isk(c.total()));
    if c.missing > 0 {
        ui.horizontal(|ui| {
            ui.add_space(rows::GAP);
            ui.add(egui::Label::new(egui::RichText::new(trf!("{n} items have no market price", n = c.missing)).weak()).selectable(false));
        });
    }
}

fn remote(ui: &mut egui::Ui, c: &Computed) {
    let r = &c.remote;
    if r.modules.is_empty() {
        return;
    }
    section(ui, tr!("Remote"));
    if r.armor_rep > 0.0 {
        line(ui, tr!("Armor repair"), trf!("{v}/s", v = text::num(r.armor_rep)));
    }
    if r.shield_rep > 0.0 {
        line(ui, tr!("Shield boost"), trf!("{v}/s", v = text::num(r.shield_rep)));
    }
    if r.hull_rep > 0.0 {
        line(ui, tr!("Hull repair"), trf!("{v}/s", v = text::num(r.hull_rep)));
    }
    if r.cap_transfer > 0.0 {
        line(ui, tr!("Capacitor given"), trf!("{v} GJ/s", v = text::num(r.cap_transfer)));
    }
    if r.neut > 0.0 {
        line(ui, tr!("Neutralised"), trf!("{v} GJ/s", v = text::num(r.neut)));
    }
    if r.rep_range > 0.0 {
        line(ui, tr!("Repair range"), text::metres(r.rep_range));
    }
    let ewar: Vec<&spai_fit::stats::RemoteModule> = r.modules.iter().filter(|m| !m.kind.is_rep() && !matches!(m.kind, spai_fit::stats::RemoteKind::Neut | spai_fit::stats::RemoteKind::Nos)).collect();
    for m in ewar {
        let Some(label) = ewar_name(m.kind) else { continue };
        line(ui, label, trf!("{v} at {o} + {f}", v = text::num(m.amount), o = text::metres(m.optimal), f = text::metres(m.falloff)));
    }
}

pub fn ewar_name(kind: spai_fit::stats::RemoteKind) -> Option<&'static str> {
    use spai_fit::stats::RemoteKind as K;
    Some(match kind {
        K::Web => tr!("Web"),
        K::Painter => tr!("Painter"),
        K::Damp => tr!("Damp"),
        K::TrackingDisruptor => tr!("Tracking disruptor"),
        K::GuidanceDisruptor => tr!("Guidance disruptor"),
        K::Ecm => tr!("ECM"),
        K::Scram => tr!("Scrambler"),
        K::Point => tr!("Disruptor"),
        K::Neut => tr!("Neutraliser"),
        K::Nos => tr!("Nosferatu"),
        _ => return None,
    })
}
