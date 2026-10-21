//! The graphs: a kind to pick, the target's figures, and the curves drawn large in the middle or
//! in a window of their own.

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
    if open.len() > 1 {
        ships_row(sim, ui, width, &open, &current, damage);
    }
    // One labelled total per attacker and target once more than one ship is on either side.
    if !sim.graph_attackers.is_empty() || sim.graph_targets.len() > 1 || (damage && !sim.graph_targets.is_empty()) {
        let plot = compare_plot(sim, ui, &spec, &open, &current, &computed, damage);
        let Some(plot) = plot else {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tr!("Nothing to draw: the fit has no module of that kind running.")).weak());
            return;
        };
        let height = (ui.available_height() - 8.0).max(200.0);
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
    let height = (ui.available_height() - 8.0).max(200.0);
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_max_width(width);
        plot.show(ui, height);
    });
}

/// Which open fits shoot and which are shot at, as two drop-down lists of the tabs.
fn ships_row(sim: &mut SimUi, ui: &mut egui::Ui, width: f32, open: &[(String, String)], current: &str, damage: bool) {
    let row = egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, 26.0))).layout(egui::Layout::left_to_right(egui::Align::Center));
    let r = ui.scope_builder(row, |ui| {
        ui.set_height(26.0);
        let menu_w = ((width - 16.0) / 2.0).clamp(120.0, 220.0);
        let attackers = trf!("Fits drawn: {n}", n = 1 + sim.graph_attackers.len());
        ui.allocate_ui_with_layout(egui::vec2(menu_w, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.menu_button(text::clip(&attackers, 28), |ui| {
                ui.set_min_width(240.0);
                for (id, name) in open {
                    let mut on = id == current || sim.graph_attackers.contains(id);
                    let r = ui.add_enabled(id != current, egui::Checkbox::new(&mut on, text::clip(name, 40)));
                    if r.changed() {
                        if on {
                            sim.graph_attackers.push(id.clone());
                        } else {
                            sim.graph_attackers.retain(|x| x != id);
                        }
                    }
                }
            });
        });
        if damage {
            let targets = if sim.graph_targets.is_empty() { tr!("Target: the profile").to_owned() } else { trf!("Targets: {n}", n = sim.graph_targets.len()) };
            ui.allocate_ui_with_layout(egui::vec2(menu_w, 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.menu_button(text::clip(&targets, 28), |ui| {
                    ui.set_min_width(240.0);
                    ui.label(egui::RichText::new(tr!("Their signature and speed stand in for the profile's.")).weak());
                    for (id, name) in open {
                        let mut on = sim.graph_targets.contains(id);
                        if ui.checkbox(&mut on, text::clip(name, 40)).changed() {
                            if on {
                                sim.graph_targets.push(id.clone());
                            } else {
                                sim.graph_targets.retain(|x| x != id);
                            }
                        }
                    }
                });
            });
        }
    });
    ui.advance_cursor_after_rect(r.response.rect);
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
