//! The graphs: a kind to pick, the target's figures, and the curves drawn large in the middle or
//! in a window of their own.

use egui_phosphor::regular as icon;
use spai_fit::graphs::{self, GraphKind, GraphSpec, GraphX, GraphY, SeriesLabel};
use spai_fit::history::EditKind;
use spai_ui::{tr, trf};

use crate::plot::{series_color, Plot, PlotSeries};
use crate::{text, SimUi};

pub fn kind_name(kind: GraphKind) -> &'static str {
    match kind {
        GraphKind::DamageVsDistance => tr!("Damage vs distance"),
        GraphKind::DamageVsTargetSpeed => tr!("Damage vs target speed"),
        GraphKind::DamageVsTargetSignature => tr!("Damage vs target signature"),
        GraphKind::VolleyVsDistance => tr!("Volley vs distance"),
        GraphKind::VolleyVsTargetSpeed => tr!("Volley vs target speed"),
        GraphKind::VolleyVsTargetSignature => tr!("Volley vs target signature"),
        GraphKind::CapacitorOverTime => tr!("Capacitor over time"),
        GraphKind::ShieldOverTime => tr!("Shield over time"),
        GraphKind::VelocityOverTime => tr!("Speed over time"),
        GraphKind::WarpTime => tr!("Warp time vs distance"),
        GraphKind::LockTime => tr!("Lock time vs signature"),
        GraphKind::RemoteRepVsDistance => tr!("Remote repair vs distance"),
        GraphKind::EwarVsDistance => tr!("Ewar vs distance"),
    }
}

type Fmt = fn(f64) -> String;

pub fn y_name(y: GraphY) -> &'static str {
    match y {
        GraphY::Dps => tr!("Damage per second"),
        GraphY::Volley => tr!("Volley"),
        GraphY::Capacitor => tr!("Capacitor"),
        GraphY::Shield => tr!("Shield HP"),
        GraphY::Speed => tr!("Speed"),
        GraphY::WarpTime => tr!("Warp time"),
        GraphY::LockTime => tr!("Lock time"),
        GraphY::RemoteRep => tr!("Remote repair"),
        GraphY::Ewar => tr!("Ewar strength"),
    }
}

pub fn x_name(x: GraphX) -> &'static str {
    match x {
        GraphX::Distance => tr!("Distance"),
        GraphX::TargetSpeed => tr!("Target speed"),
        GraphX::TargetSignature => tr!("Target signature"),
        GraphX::Time => tr!("Time"),
        GraphX::WarpDistance => tr!("Warp distance"),
    }
}

fn axes(kind: GraphKind) -> (&'static str, &'static str, Fmt, Fmt) {
    let km: Fmt = text::metres;
    let s: Fmt = |v| format!("{v:.0} s");
    let au: Fmt = |v| format!("{v:.0} AU");
    let plain: Fmt = text::num;
    match kind {
        GraphKind::DamageVsDistance => (tr!("Distance"), tr!("DPS"), km, plain),
        GraphKind::DamageVsTargetSpeed => (tr!("Target speed (m/s)"), tr!("DPS"), plain, plain),
        GraphKind::DamageVsTargetSignature => (tr!("Target signature (m)"), tr!("DPS"), plain, plain),
        GraphKind::VolleyVsDistance => (tr!("Distance"), tr!("Volley"), km, plain),
        GraphKind::VolleyVsTargetSpeed => (tr!("Target speed (m/s)"), tr!("Volley"), plain, plain),
        GraphKind::VolleyVsTargetSignature => (tr!("Target signature (m)"), tr!("Volley"), plain, plain),
        GraphKind::CapacitorOverTime => (tr!("Time"), tr!("GJ"), s, plain),
        GraphKind::ShieldOverTime => (tr!("Time"), tr!("Shield HP"), s, plain),
        GraphKind::VelocityOverTime => (tr!("Time"), tr!("m/s"), s, plain),
        GraphKind::WarpTime => (tr!("Distance"), tr!("Seconds"), au, plain),
        GraphKind::LockTime => (tr!("Target signature (m)"), tr!("Seconds"), plain, plain),
        GraphKind::RemoteRepVsDistance => (tr!("Distance"), tr!("Per second"), km, plain),
        GraphKind::EwarVsDistance => (tr!("Distance"), tr!("Strength"), km, plain),
    }
}

/// The graphs filling the space given: the controls on top, the plot below.
pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| body(sim, ui, now_ms));
}

/// A control of the target row: a label, a slider with no value of its own, and the value in a
/// box of fixed width beside it, so a large number never pushes the row into wrapping.
fn control(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, log: bool, fmt: impl Fn(f64) -> String, parse: impl Fn(&str) -> Option<f64>) -> bool {
    const LABEL_W: f32 = 104.0;
    const VALUE_W: f32 = 96.0;
    // At most the row's width, so a narrow window shrinks the slider instead of the row
    // pushing past the edge.
    let cell = egui::vec2(330.0f32.min(ui.max_rect().width()).max(LABEL_W + VALUE_W + 60.0), 26.0);
    let mut changed = false;
    ui.allocate_ui_with_layout(cell, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.allocate_ui_with_layout(egui::vec2(LABEL_W, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(LABEL_W);
            ui.add(egui::Label::new(label).truncate().selectable(false));
        });
        ui.spacing_mut().slider_width = cell.x - LABEL_W - VALUE_W - 2.0 * ui.spacing().item_spacing.x - 4.0;
        let slider = egui::Slider::new(value, range.clone()).show_value(false);
        let slider = if log { slider.logarithmic(true) } else { slider };
        changed |= ui.add(slider).changed();
        let mut v = f64::from(*value);
        let (lo, hi) = (f64::from(*range.start()), f64::from(*range.end()));
        let r = ui.add_sized([VALUE_W, 20.0], egui::DragValue::new(&mut v).range(lo..=hi).custom_formatter(|x, _| fmt(x)).custom_parser(|s| parse(s)));
        if r.changed() {
            *value = v as f32;
            changed = true;
        }
    });
    changed
}

/// A number with an optional unit after it, as typed into a value box.
fn leading_number(s: &str) -> Option<f64> {
    let t: String = s.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    t.parse().ok()
}

fn body(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    // Taken before the controls: whatever they need, the plot is as wide as the space given.
    let width = ui.available_width();
    let Some(computed) = sim.computed() else {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("No fit open.")).weak());
        return;
    };
    let kind = sim.graph;
    let mut target = sim.session().map(|s| s.fit.target.clone()).unwrap_or_default();
    let before = target.clone();
    let damage = kind.is_damage();
    // Wide enough, the target's settings take a column right of the plot.
    let wide = width >= 800.0;
    // A fixed height above the plot whatever it holds, so nothing in it moves the plot: one line
    // wide, room for the target's settings, scrolled if need be, when narrow.
    let one_line = width >= 880.0;
    let top_h = if wide || !damage { if one_line { 30.0 } else { 62.0 } } else { 150.0 };
    let (top, _) = ui.allocate_exact_size(egui::vec2(width, top_h), egui::Sense::hover());
    let mut top_ui = ui.new_child(egui::UiBuilder::new().max_rect(top).layout(egui::Layout::top_down(egui::Align::Min)));
    top_ui.set_clip_rect(top_ui.clip_rect().intersect(top));
    egui::ScrollArea::vertical().id_salt("sim_graph_top").max_height(top_h).auto_shrink([false, true]).show(&mut top_ui, |ui| {
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
        ui.horizontal_wrapped(|ui| {
        ui.set_max_width(width);
        // The side and the bottom picked apart; a bottom that does not go with the side is
        // offered greyed, and a side that loses its bottom takes its first one.
        let (y, x) = kind.axes();
        let combo_w = 190.0f32.min(width);
        ui.allocate_ui_with_layout(egui::vec2(combo_w + 160.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(tr!("Plot"));
            egui::ComboBox::from_id_salt("sim_graph_y").selected_text(y_name(y)).width(combo_w - 50.0).show_ui(ui, |ui| {
                use spai_ui::widgets::SteadySelect as _;
                for cand in GraphY::ALL {
                    if ui.menu_label(cand == y, y_name(cand)).clicked() && cand != y {
                        let x = if cand.xs().contains(&x) { x } else { cand.xs()[0] };
                        if let Some(k) = GraphKind::from_axes(cand, x) {
                            sim.graph = k;
                        }
                    }
                }
            });
        });
        ui.allocate_ui_with_layout(egui::vec2(combo_w + 40.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(tr!("against"));
            egui::ComboBox::from_id_salt("sim_graph_x").selected_text(x_name(x)).width(combo_w - 30.0).show_ui(ui, |ui| {
                use spai_ui::widgets::SteadySelect as _;
                for cand in GraphX::ALL {
                    let fits = GraphKind::from_axes(y, cand);
                    let r = ui.add_enabled_ui(fits.is_some(), |ui| ui.menu_label(cand == x, x_name(cand))).inner;
                    let r = r.on_disabled_hover_text(trf!("{y} does not run against {x}", y = y_name(y), x = x_name(cand)));
                    if let (true, Some(k)) = (r.clicked(), fits) {
                        sim.graph = k;
                    }
                }
            });
        });
        range_box(ui, "sim_graph_x_to", tr!("X up to"), &mut sim.graph_x_to, x, x_fmt(x), x_parse(x), x_step(x));
        range_box(ui, "sim_graph_y_to", tr!("Y up to"), &mut sim.graph_y_to, y, |v| text::num(v), leading_number, 1.0);
        if damage && !wide {
            target_controls(sim, ui, kind, &mut target, false);
        }
    });
    });
    ui.add_space(4.0);
    // The rest of the space, split: the ships' column on the left, the plot, and the target's
    // settings on the right.
    let (area, _) = ui.allocate_exact_size(egui::vec2(width, (ui.available_height() - 8.0).max(200.0)), egui::Sense::hover());
    let right_w = if wide && damage { CONTROLS_W } else { 0.0 };
    let open: Vec<(String, String)> = sim.sessions().iter().map(|s| (s.id.clone(), s.fit.name.clone())).collect();
    let current = sim.session().map(|s| s.id.clone()).unwrap_or_default();
    sim.graph_attackers.retain(|id| *id != current && open.iter().any(|(o, _)| o == id));
    sim.graph_targets.retain(|id| open.iter().any(|(o, _)| o == id));
    // Both columns only where the plot keeps room; else the ships head the right one.
    let ships_right = right_w > 0.0 && width - right_w - SHIPS_W - 16.0 < 480.0;
    if right_w > 0.0 {
        let rect = egui::Rect::from_min_max(egui::pos2(area.max.x - right_w, area.min.y), area.max);
        let mut col = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
        col.set_clip_rect(col.clip_rect().intersect(rect));
        egui::Frame::NONE.fill(col.visuals().faint_bg_color).corner_radius(4.0).inner_margin(6).show(&mut col, |ui| {
            ui.set_width(right_w - 12.0);
            ui.set_min_height(rect.height() - 12.0);
            egui::ScrollArea::vertical().id_salt("sim_graph_controls").auto_shrink([false, false]).show(ui, |ui| {
                if ships_right {
                    ships_list(sim, ui, &open, &current, damage);
                    ui.add_space(10.0);
                }
                target_controls(sim, ui, kind, &mut target, true);
            });
        });
    }
    if target != before {
        let t = target.clone();
        sim.edit(EditKind::Target, now_ms, move |f| f.target = t);
    }
    let shooter_top = computed.mobility.max_velocity;
    let motion = sim.graph_motion;
    let spec = GraphSpec { include_drones: sim.graph_drones, x_to: sim.graph_x_to.get(&kind.axes().1).copied(), ..GraphSpec::new(kind, motion.apply(&target, shooter_top)) };
    let area = egui::Rect::from_min_max(area.min, egui::pos2(area.max.x - if right_w > 0.0 { right_w + 8.0 } else { 0.0 }, area.max.y));
    let width = area.width();
    let col_w = if !ships_right && width >= 640.0 { SHIPS_W } else { 0.0 };
    if col_w > 0.0 {
        let mut col = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(area.min, egui::vec2(col_w, area.height()))).layout(egui::Layout::top_down(egui::Align::Min)));
        ships_panel(sim, &mut col, &open, &current, damage);
    }
    let plot_rect = egui::Rect::from_min_max(area.min + egui::vec2(if col_w > 0.0 { col_w + 8.0 } else { 0.0 }, 0.0), area.max);
    let mut plot_ui = ui.new_child(egui::UiBuilder::new().max_rect(plot_rect).layout(egui::Layout::top_down(egui::Align::Min)));
    let ui = &mut plot_ui;
    let width = plot_rect.width();
    if !sim.graph_current && sim.graph_attackers.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("Add a ship to shoot with, on the left.")).weak());
        return;
    }
    if damage && !sim.graph_ideal && sim.graph_targets.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("Add a target, on the left.")).weak());
        return;
    }
    // One labelled total per shooter and target once the graph holds more than the fit on screen
    // against the profile; that one alone is drawn weapon by weapon.
    let alone = sim.graph_current && sim.graph_attackers.is_empty() && (!damage || (sim.graph_ideal && sim.graph_targets.is_empty()));
    if !alone {
        let plot = compare_plot(sim, ui, &spec, &target, motion, &open, &current, &computed, damage);
        let Some(plot) = plot else {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tr!("Nothing to draw: the fit has no module of that kind running.")).weak());
            return;
        };
        let height = ui.available_height().max(200.0);
        ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(width);
            plot.show(ui, height);
        });
        return;
    }
    let cap = sim.cap_inputs();
    let series = graphs::sample(&computed, &spec, cap.as_ref());
    if series.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("Nothing to draw: the fit has no module of that kind running.")).weak());
        return;
    }
    let (x_label, y_label, fmt_x, fmt_y) = axes(kind);
    let plot = Plot {
        x_label: x_label.to_owned(),
        y_label: y_label.to_owned(),
        fmt_x,
        fmt_y,
        y_to: sim.graph_y_to.get(&kind.axes().0).copied(),
        series: series
            .iter()
            .enumerate()
            .map(|(i, s)| PlotSeries {
                label: match &s.label {
                    SeriesLabel::Weapon(t, c) => {
                        let name = dogma.type_(*t).map_or_else(|| trf!("Type {id}", id = t), |d| d.name.clone());
                        match c.and_then(|c| dogma.type_(c)) {
                            Some(ch) => format!("{} ({})", text::clip(&name, 26), text::clip(&ch.name, 18)),
                            None => text::clip(&name, 36),
                        }
                    }
                    SeriesLabel::Total => tr!("Total").to_owned(),
                    SeriesLabel::Capacitor => tr!("Capacitor").to_owned(),
                    SeriesLabel::Shield => tr!("Shield").to_owned(),
                    SeriesLabel::Velocity => tr!("Speed").to_owned(),
                    SeriesLabel::Warp => tr!("Warp time").to_owned(),
                    SeriesLabel::Lock => tr!("Lock time").to_owned(),
                    SeriesLabel::Remote(t, kind) => {
                        let name = dogma.type_(*t).map_or_else(|| trf!("Type {id}", id = t), |d| d.name.clone());
                        format!("{} ({})", text::clip(&name, 30), remote_unit(*kind))
                    }
                },
                color: if s.label == SeriesLabel::Total { ui.visuals().text_color() } else { series_color(i) },
                points: s.points.clone(),
            })
            .collect(),
    };
    let height = ui.available_height().max(200.0);
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_max_width(width);
        plot.show(ui, height);
    });
}

/// Each ship's share of its top speed and its heading in degrees against the line to the other,
/// 90 straight across. By default the shooter is still and the target crosses at full speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub shooter: (f32, f32),
    pub target: (f32, f32),
}

impl Default for Motion {
    fn default() -> Self {
        Motion { shooter: (0.0, 90.0), target: (1.0, 90.0) }
    }
}

impl Motion {
    /// The profile moving so: the target's speed its share of the profile's, the transversal what
    /// the two headings leave across the line between them.
    pub fn apply(&self, t: &spai_fit::fit::TargetProfile, shooter_top: f64) -> spai_fit::fit::TargetProfile {
        let va = f64::from(self.shooter.0) * shooter_top;
        let vt = f64::from(self.target.0) * f64::from(t.velocity);
        let across = va * f64::from(self.shooter.1).to_radians().sin() - vt * f64::from(self.target.1).to_radians().sin();
        spai_fit::fit::TargetProfile { velocity: vt as f32, transversal: across.abs() as f32, ..t.clone() }
    }

    /// The target's share of speed that gives `angular` at the profile's distance, headings kept;
    /// as close as the circle allows when it cannot.
    pub fn solve_target_share(&mut self, t: &spai_fit::fit::TargetProfile, shooter_top: f64, angular: f64) {
        let va = f64::from(self.shooter.0) * shooter_top * f64::from(self.shooter.1).to_radians().sin();
        let per = f64::from(t.velocity) * f64::from(self.target.1).to_radians().sin();
        let want = angular * f64::from(t.distance).max(1.0);
        if per.abs() < 1e-9 {
            return;
        }
        // Either side of the shooter's own sweep gives the same transversal; the smaller share wins.
        let shares = [(va - want) / per, (va + want) / per];
        let best = shares.into_iter().filter(|f| (0.0..=1.0).contains(f)).fold(None, |b: Option<f64>, f| Some(b.map_or(f, |b| b.min(f))));
        self.target.0 = best.unwrap_or_else(|| shares.into_iter().map(|f| f.clamp(0.0, 1.0)).fold(0.0, f64::max)) as f32;
    }
}

/// An axis's end: "Auto" ticked follows the data, unticked a value typed or dragged.
#[allow(clippy::too_many_arguments)]
fn range_box<K: std::hash::Hash + Eq + Copy>(ui: &mut egui::Ui, id: &str, label: &str, map: &mut std::collections::HashMap<K, f64>, key: K, fmt: fn(f64) -> String, parse: fn(&str) -> Option<f64>, step: f64) {
    ui.allocate_ui_with_layout(egui::vec2(260.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.push_id(id, |ui| {
            ui.label(label);
            let mut auto = !map.contains_key(&key);
            let mut v = map.get(&key).copied().unwrap_or(step * 100.0);
            let r = ui.add_enabled_ui(!auto, |ui| ui.add_sized([96.0, 20.0], egui::DragValue::new(&mut v).range(step..=f64::MAX).speed(step).custom_formatter(|x, _| fmt(x)).custom_parser(parse))).inner;
            if r.changed() {
                map.insert(key, v);
            }
            if ui.checkbox(&mut auto, tr!("Auto")).changed() {
                if auto {
                    map.remove(&key);
                } else {
                    map.insert(key, v);
                }
            }
        });
    });
}

fn x_fmt(x: GraphX) -> fn(f64) -> String {
    match x {
        GraphX::Distance => |v| format!("{} km", text::num(v / 1000.0)),
        GraphX::TargetSpeed => |v| format!("{v:.0} m/s"),
        GraphX::TargetSignature => |v| format!("{v:.0} m"),
        GraphX::Time => |v| format!("{v:.0} s"),
        GraphX::WarpDistance => |v| format!("{v:.1} AU"),
    }
}

fn x_parse(x: GraphX) -> fn(&str) -> Option<f64> {
    match x {
        // Kilometres unless metres are said.
        GraphX::Distance => |s| leading_number(s).map(|n| if s.trim().ends_with(" m") || (s.trim().ends_with('m') && !s.trim().ends_with("km")) { n } else { n * 1000.0 }),
        _ => leading_number,
    }
}

fn x_step(x: GraphX) -> f64 {
    match x {
        GraphX::Distance => 1000.0,
        GraphX::TargetSpeed | GraphX::TargetSignature => 10.0,
        GraphX::Time => 1.0,
        GraphX::WarpDistance => 0.1,
    }
}

const CONTROLS_W: f32 = 230.0;

/// The target's signature, speed and distance, the motion of both ships and whether drones count:
/// in a row above the plot, or stacked in a column of their own.
fn target_controls(sim: &mut SimUi, ui: &mut egui::Ui, kind: GraphKind, target: &mut spai_fit::fit::TargetProfile, stacked: bool) {
    let metres = |v: f64| text::metres(v);
    let parse_metres = |s: &str| leading_number(s).map(|n| if s.trim().ends_with("km") { n * 1000.0 } else { n });
    let (_, x) = kind.axes();
    let ctl = |ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, log: bool, fmt: &dyn Fn(f64) -> String, parse: &dyn Fn(&str) -> Option<f64>| {
        if stacked {
            control_stacked(ui, label, value, range, log, fmt, parse)
        } else {
            control(ui, label, value, range, log, fmt, parse)
        }
    };
    if x != GraphX::TargetSignature {
        ctl(ui, tr!("Target signature"), &mut target.signature, 25.0..=2000.0, true, &|v| format!("{v:.0} m"), &leading_number);
    }
    if x != GraphX::TargetSpeed {
        ctl(ui, tr!("Target speed"), &mut target.velocity, 0.0..=5000.0, false, &|v| format!("{v:.0} m/s"), &leading_number);
    }
    if x != GraphX::Distance {
        ctl(ui, tr!("Distance"), &mut target.distance, 0.0..=250_000.0, false, &metres, &parse_metres);
    }
    if x != GraphX::TargetSpeed {
        if stacked {
            ui.add_space(6.0);
        }
        ui.allocate_ui_with_layout(egui::vec2(214.0, 120.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
            let m = &mut sim.graph_motion;
            motion_dial(ui, tr!("Shooter"), &mut m.shooter);
            motion_dial(ui, tr!("Target"), &mut m.target);
        });
        let shooter_v = sim.computed().map_or(0.0, |c| c.mobility.max_velocity);
        let t = sim.graph_motion.apply(target, shooter_v);
        // The circles and this one figure are the same setting: moving it rescales the target's
        // share of speed, moving a circle moves it.
        let mut angular = (f64::from(t.transversal) / f64::from(t.distance).max(1.0)) as f32;
        if ctl(ui, tr!("Angular velocity"), &mut angular, 0.0..=2.0, false, &|v| format!("{v:.3} rad/s"), &leading_number) {
            sim.graph_motion.solve_target_share(target, shooter_v, f64::from(angular));
        }
    }
    ui.allocate_ui_with_layout(egui::vec2(160.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.checkbox(&mut sim.graph_drones, tr!("Include drones"));
    });
}

/// A label over a slider with its value box, as wide as the column.
fn control_stacked(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, log: bool, fmt: &dyn Fn(f64) -> String, parse: &dyn Fn(&str) -> Option<f64>) -> bool {
    const VALUE_W: f32 = 84.0;
    let mut changed = false;
    ui.label(label);
    let w = ui.available_width();
    ui.allocate_ui_with_layout(egui::vec2(w, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().slider_width = (w - VALUE_W - ui.spacing().item_spacing.x - 2.0).max(40.0);
        let slider = egui::Slider::new(value, range.clone()).show_value(false);
        let slider = if log { slider.logarithmic(true) } else { slider };
        changed |= ui.add(slider).changed();
        let mut v = f64::from(*value);
        let (lo, hi) = (f64::from(*range.start()), f64::from(*range.end()));
        if ui.add_sized([VALUE_W, 20.0], egui::DragValue::new(&mut v).range(lo..=hi).custom_formatter(|x, _| fmt(x)).custom_parser(|s| parse(s))).changed() {
            *value = v as f32;
            changed = true;
        }
    });
    ui.add_space(4.0);
    changed
}

/// A ship's motion: the circle is its top speed, the point how much of it it
/// makes and in which direction, 90 degrees across the line to the other ship, upwards.
fn motion_dial(ui: &mut egui::Ui, label: &str, v: &mut (f32, f32)) {
    const D: f32 = 100.0;
    ui.vertical(|ui| {
        ui.set_width(D + 4.0);
        ui.vertical_centered(|ui| ui.label(egui::RichText::new(label).weak()));
        let (rect, r) = ui.allocate_exact_size(egui::vec2(D + 4.0, D + 4.0), egui::Sense::click_and_drag());
        let r = r.on_hover_text(tr!("Drag to set the heading and the share of top speed."));
        let c = rect.center();
        let radius = D / 2.0;
        if let Some(p) = r.interact_pointer_pos().filter(|_| r.dragged() || r.clicked()) {
            let d = p - c;
            let frac = (d.length() / radius).clamp(0.0, 1.0);
            let deg = (-d.y).atan2(d.x).to_degrees().rem_euclid(360.0);
            *v = ((frac * 20.0).round() / 20.0, (deg / 5.0).round() * 5.0 % 360.0);
        }
        let stroke = egui::Stroke::new(1.5, ui.visuals().text_color());
        let painter = ui.painter_at(rect);
        painter.circle_stroke(c, radius, stroke);
        let a = v.1.to_radians();
        let tip = c + egui::vec2(a.cos(), -a.sin()) * radius * v.0;
        if v.0 > 0.0 {
            painter.line_segment([c, tip], stroke);
        }
        painter.circle_filled(tip, 3.5, ui.visuals().text_color());
        let font = egui::TextStyle::Body.resolve(ui.style());
        painter.text(c + egui::vec2(-radius * 0.55, -radius * 0.45), egui::Align2::CENTER_CENTER, format!("{:.0}%", v.0 * 100.0), font.clone(), ui.visuals().text_color());
        painter.text(c + egui::vec2(0.0, radius * 0.55), egui::Align2::CENTER_CENTER, format!("{:.0}°", v.1), font, ui.visuals().text_color());
    });
}

const SHIPS_W: f32 = 230.0;

const LINES: &str = "sim_compare_lines";

/// How many lines the last comparison plot drew, for tests.
pub fn last_line_count(ctx: &egui::Context) -> usize {
    ctx.data(|d| d.get_temp(egui::Id::new(LINES))).unwrap_or(0)
}

/// The graph's ships, in a column of their own left of the plot: the shooters and, for damage, the
/// targets, each with a button to add more. The graph belongs to no fit: it starts with the fit
/// on screen against the target profile, and either can be taken out.
fn ships_panel(sim: &mut SimUi, ui: &mut egui::Ui, open: &[(String, String)], current: &str, damage: bool) {
    let rect = ui.max_rect();
    let height = rect.height();
    ui.set_clip_rect(ui.clip_rect().intersect(rect));
    egui::Frame::NONE.fill(ui.visuals().faint_bg_color).corner_radius(4.0).inner_margin(6).show(ui, |ui| {
        ui.set_width(SHIPS_W - 12.0);
        ui.set_min_height(height - 12.0);
        egui::ScrollArea::vertical().id_salt("sim_graph_ships").auto_shrink([false, false]).show(ui, |ui| ships_list(sim, ui, open, current, damage));
    });
}

/// The shooters and the targets, each with its button to add more.
fn ships_list(sim: &mut SimUi, ui: &mut egui::Ui, open: &[(String, String)], current: &str, damage: bool) {
    let name_of = |id: &str| open.iter().find(|(o, _)| o == id).map(|(_, n)| n.clone()).unwrap_or_default();
    {
        {
            {
                let row = |ui: &mut egui::Ui, label: String| -> bool {
                    let mut remove = false;
                    let w = ui.available_width();
                    ui.allocate_ui_with_layout(egui::vec2(w, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_width(w);
                        ui.add_space(4.0);
                        let name_w = w - 4.0 - 24.0 - ui.spacing().item_spacing.x;
                        crate::rows::cell(ui, name_w, egui::RichText::new(&label), &label);
                        remove = spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Take it off the graph")).clicked();
                    });
                    remove
                };
                ui.label(egui::RichText::new(tr!("Shooting")).strong());
                if sim.graph_current && row(ui, trf!("{name} (on screen)", name = name_of(current))) {
                    sim.graph_current = false;
                }
                for id in sim.graph_attackers.clone() {
                    if row(ui, name_of(&id)) {
                        sim.graph_attackers.retain(|x| *x != id);
                    }
                }
                ui.menu_button(format!("{} {}", icon::PLUS, tr!("Add a shooter")), |ui| {
                    ui.set_min_width(240.0);
                    if !sim.graph_current && ui.button(tr!("The fit on screen")).clicked() {
                        sim.graph_current = true;
                        ui.close();
                    }
                    let mut any = false;
                    let taken = sim.graph_attackers.clone();
                    for (id, name) in open.iter().filter(|(id, _)| id != current && !taken.contains(id)) {
                        any = true;
                        if ui.button(text::clip(name, 40)).clicked() {
                            sim.graph_attackers.push(id.clone());
                            ui.close();
                        }
                    }
                    if !any && sim.graph_current {
                        ui.label(egui::RichText::new(tr!("Open more fits in tabs to draw them here.")).weak());
                    }
                });
                if !damage {
                    return;
                }
                ui.add_space(10.0);
                ui.label(egui::RichText::new(tr!("Targets")).strong());
                if sim.graph_ideal && row(ui, tr!("Target profile").to_owned()) {
                    sim.graph_ideal = false;
                }
                for id in sim.graph_targets.clone() {
                    if row(ui, name_of(&id)) {
                        sim.graph_targets.retain(|x| *x != id);
                    }
                }
                ui.menu_button(format!("{} {}", icon::PLUS, tr!("Add a target")), |ui| {
                    ui.set_min_width(240.0);
                    if !sim.graph_ideal && ui.button(tr!("Target profile")).clicked() {
                        sim.graph_ideal = true;
                        ui.close();
                    }
                    ui.label(egui::RichText::new(tr!("A fit's signature and speed stand in for the profile's.")).weak());
                    let taken = sim.graph_targets.clone();
                    for (id, name) in open.iter().filter(|(id, _)| !taken.contains(id)) {
                        if ui.button(text::clip(name, 40)).clicked() {
                            sim.graph_targets.push(id.clone());
                            ui.close();
                        }
                    }
                });
            }
        }
    }
}

/// One line per attacker (and per target, for damage), each the total of that fit's weapons.
fn compare_plot(sim: &mut SimUi, ui: &egui::Ui, spec: &GraphSpec, raw: &spai_fit::fit::TargetProfile, motion: Motion, open: &[(String, String)], current: &str, computed: &std::sync::Arc<spai_fit::stats::Computed>, damage: bool) -> Option<Plot> {
    let name_of = |id: &str| open.iter().find(|(o, _)| o == id).map(|(_, n)| n.clone()).unwrap_or_default();
    // The fit on screen shoots only while it is on the list; it can be taken off like any other.
    let mut attackers: Vec<(String, std::sync::Arc<spai_fit::stats::Computed>)> = if sim.graph_current { vec![(name_of(current), computed.clone())] } else { Vec::new() };
    for id in sim.graph_attackers.clone() {
        if let Some(c) = sim.computed_of(&id) {
            attackers.push((name_of(&id), c));
        }
    }
    let mut targets: Vec<(Option<String>, spai_fit::fit::TargetProfile)> = Vec::new();
    if damage && !sim.graph_targets.is_empty() {
        if sim.graph_ideal {
            targets.push((Some(tr!("Target profile").to_owned()), raw.clone()));
        }
        for id in sim.graph_targets.clone() {
            if let Some(c) = sim.computed_of(&id) {
                let t = spai_fit::fit::TargetProfile { signature: c.mobility.signature as f32, velocity: c.mobility.max_velocity as f32, ..raw.clone() };
                targets.push((Some(name_of(&id)), t));
            }
        }
    } else {
        targets.push((None, raw.clone()));
    }
    let mut lines: Vec<PlotSeries> = Vec::new();
    for (a_name, c) in &attackers {
        for (t_name, t) in &targets {
            let one = GraphSpec { target: motion.apply(t, c.mobility.max_velocity), ..spec.clone() };
            let series = graphs::sample(c, &one, None);
            let total = series.iter().find(|s| s.label == SeriesLabel::Total).or_else(|| series.first());
            let Some(total) = total else { continue };
            let label = match t_name {
                Some(t) => format!("{} → {}", text::clip(a_name, 22), text::clip(t, 22)),
                None => text::clip(a_name, 40),
            };
            lines.push(PlotSeries { label, color: series_color(lines.len()), points: total.points.clone() });
        }
    }
    if lines.is_empty() {
        return None;
    }
    let (x_label, y_label, fmt_x, fmt_y) = axes(spec.kind);
    let _ = ui;
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(LINES), lines.len()));
    Some(Plot { x_label: x_label.to_owned(), y_label: y_label.to_owned(), fmt_x, fmt_y, y_to: sim.graph_y_to.get(&spec.kind.axes().0).copied(), series: lines })
}

/// The graphs in a window of their own while `graph_window` is on: a window of the desktop's
/// own on the desktop, a floating one where the host cannot open another (the browser, tests).
pub fn window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    if !sim.graph_window {
        return;
    }
    let size = egui::vec2(760.0, 480.0);
    if ctx.embed_viewports() {
        let mut open = true;
        egui::Window::new(tr!("Graphs")).id(egui::Id::new("sim_graph_window")).open(&mut open).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
            show(sim, ui, now_ms);
        });
        sim.graph_window = open;
        return;
    }
    let builder = egui::ViewportBuilder::default().with_title(tr!("Graphs")).with_inner_size(size).with_min_inner_size([480.0, 320.0]);
    ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("sim_graph_window"), builder, |ctx, _| {
        // The root of a viewport of its own has no Ui to show inside, as the app's windows do it.
        #[allow(deprecated)]
        egui::CentralPanel::default().frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill)).show(ctx, |ui| show(sim, ui, now_ms));
        if ctx.input(|i| i.viewport().close_requested()) {
            sim.graph_window = false;
        }
    });
}

/// What a remote curve's numbers are.
fn remote_unit(kind: spai_fit::stats::RemoteKind) -> &'static str {
    use spai_fit::stats::RemoteKind as K;
    match kind {
        K::ArmorRep => tr!("armor HP/s"),
        K::ShieldRep => tr!("shield HP/s"),
        K::HullRep => tr!("hull HP/s"),
        K::CapTransfer | K::Nos => tr!("GJ/s given"),
        K::Neut => tr!("GJ/s drained"),
        K::Web | K::Painter | K::Damp | K::TrackingDisruptor | K::GuidanceDisruptor => tr!("%"),
        K::Ecm => tr!("jam points"),
        K::Scram | K::Point => tr!("warp points"),
    }
}
