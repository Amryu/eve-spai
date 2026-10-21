//! The graphs: a kind to pick, the target's figures, and the curves drawn large in the middle or
//! in a window of their own.

use egui_phosphor::regular as icon;
use spai_fit::graphs::{self, GraphKind, GraphSpec, SeriesLabel};
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
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
        ui.horizontal_wrapped(|ui| {
        ui.set_max_width(width);
        let combo_w = 250.0f32.min(width);
        ui.allocate_ui_with_layout(egui::vec2(combo_w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt("sim_graph_kind").selected_text(kind_name(kind)).width(combo_w - 10.0).show_ui(ui, |ui| {
                use spai_ui::widgets::SteadySelect as _;
                for k in GraphKind::ALL {
                    ui.menu_value(&mut sim.graph, k, kind_name(k));
                }
            });
        });
        if damage {
            let metres = |v: f64| text::metres(v);
            let parse_metres = |s: &str| leading_number(s).map(|n| if s.trim().ends_with("km") { n * 1000.0 } else { n });
            if !matches!(kind, GraphKind::DamageVsTargetSignature | GraphKind::VolleyVsTargetSignature) {
                control(ui, tr!("Target signature"), &mut target.signature, 25.0..=2000.0, true, |v| format!("{v:.0} m"), leading_number);
            }
            if !matches!(kind, GraphKind::DamageVsTargetSpeed | GraphKind::VolleyVsTargetSpeed) {
                control(ui, tr!("Target speed"), &mut target.velocity, 0.0..=5000.0, false, |v| format!("{v:.0} m/s"), leading_number);
                // What tracking is measured against is the angle the target sweeps per second,
                // so that is the figure shown; the profile keeps the transversal it implies at
                // the chosen distance.
                let distance = f64::from(target.distance).max(1.0);
                let mut angular = (f64::from(target.transversal) / distance) as f32;
                if control(ui, tr!("Angular velocity"), &mut angular, 0.0..=2.0, false, |v| format!("{v:.3} rad/s"), leading_number) {
                    target.transversal = (f64::from(angular) * distance) as f32;
                }
            }
            if !matches!(kind, GraphKind::DamageVsDistance | GraphKind::VolleyVsDistance) {
                control(ui, tr!("Distance"), &mut target.distance, 0.0..=250_000.0, false, metres, parse_metres);
            }
            ui.allocate_ui_with_layout(egui::vec2(160.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.checkbox(&mut sim.graph_drones, tr!("Include drones"));
            });
        }
    });
    });
    if target != before {
        let t = target.clone();
        sim.edit(EditKind::Target, now_ms, move |f| f.target = t);
    }
    let spec = GraphSpec { include_drones: sim.graph_drones, ..GraphSpec::new(kind, target) };
    let open: Vec<(String, String)> = sim.sessions().iter().map(|s| (s.id.clone(), s.fit.name.clone())).collect();
    let current = sim.session().map(|s| s.id.clone()).unwrap_or_default();
    sim.graph_attackers.retain(|id| *id != current && open.iter().any(|(o, _)| o == id));
    sim.graph_targets.retain(|id| open.iter().any(|(o, _)| o == id));
    ui.add_space(4.0);
    // The rest of the space, split: the ships' column on the left, the plot right of it.
    let (area, _) = ui.allocate_exact_size(egui::vec2(width, (ui.available_height() - 8.0).max(200.0)), egui::Sense::hover());
    let col_w = if width >= 640.0 { SHIPS_W } else { 0.0 };
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
        let plot = compare_plot(sim, ui, &spec, &open, &current, &computed, damage);
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

const SHIPS_W: f32 = 230.0;

/// The graph's ships, in a column of their own left of the plot: the shooters and, for damage, the
/// targets, each with a button to add more. The graph belongs to no fit: it starts with the fit
/// on screen against the target profile, and either can be taken out.
fn ships_panel(sim: &mut SimUi, ui: &mut egui::Ui, open: &[(String, String)], current: &str, damage: bool) {
    const W: f32 = SHIPS_W;
    let name_of = |id: &str| open.iter().find(|(o, _)| o == id).map(|(_, n)| n.clone()).unwrap_or_default();
    let rect = ui.max_rect();
    let height = rect.height();
    ui.set_clip_rect(ui.clip_rect().intersect(rect));
    {
        egui::Frame::NONE.fill(ui.visuals().faint_bg_color).corner_radius(4.0).inner_margin(6).show(ui, |ui| {
            ui.set_width(W - 12.0);
            ui.set_min_height(height - 12.0);
            egui::ScrollArea::vertical().id_salt("sim_graph_ships").auto_shrink([false, false]).show(ui, |ui| {
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
            });
        });
    }
}

/// One line per attacker (and per target, for damage), each the total of that fit's weapons.
fn compare_plot(sim: &mut SimUi, ui: &egui::Ui, spec: &GraphSpec, open: &[(String, String)], current: &str, computed: &std::sync::Arc<spai_fit::stats::Computed>, damage: bool) -> Option<Plot> {
    let name_of = |id: &str| open.iter().find(|(o, _)| o == id).map(|(_, n)| n.clone()).unwrap_or_default();
    let mut attackers: Vec<(String, std::sync::Arc<spai_fit::stats::Computed>)> = vec![(name_of(current), computed.clone())];
    for id in sim.graph_attackers.clone() {
        if let Some(c) = sim.computed_of(&id) {
            attackers.push((name_of(&id), c));
        }
    }
    let mut targets: Vec<(Option<String>, spai_fit::fit::TargetProfile)> = Vec::new();
    if damage && !sim.graph_targets.is_empty() {
        if sim.graph_ideal {
            targets.push((Some(tr!("Target profile").to_owned()), spec.target.clone()));
        }
        for id in sim.graph_targets.clone() {
            if let Some(c) = sim.computed_of(&id) {
                let t = spai_fit::fit::TargetProfile { signature: c.mobility.signature as f32, velocity: c.mobility.max_velocity as f32, ..spec.target.clone() };
                targets.push((Some(name_of(&id)), t));
            }
        }
    } else {
        targets.push((None, spec.target.clone()));
    }
    let mut lines: Vec<PlotSeries> = Vec::new();
    for (a_name, c) in &attackers {
        for (t_name, t) in &targets {
            let one = GraphSpec { target: t.clone(), ..spec.clone() };
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
    Some(Plot { x_label: x_label.to_owned(), y_label: y_label.to_owned(), fmt_x, fmt_y, series: lines })
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
