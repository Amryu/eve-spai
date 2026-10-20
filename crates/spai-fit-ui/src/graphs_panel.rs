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
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(computed) = sim.computed() else {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr!("No fit open.")).weak());
        return;
    };
    let kind = sim.graph;
    let mut target = sim.session.as_ref().map(|s| s.fit.target.clone()).unwrap_or_default();
    let before = target.clone();
    let damage = matches!(kind, GraphKind::DamageVsDistance | GraphKind::DamageVsTargetSpeed | GraphKind::DamageVsTargetSignature);
    // Each control a cell of known size, so the row wraps whole controls, value box included.
    let cell = egui::vec2(330.0, 26.0);
    let slider_w = 150.0;
    let slider = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.allocate_ui_with_layout(cell, egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.spacing_mut().slider_width = slider_w;
            ui.add_sized([104.0, 20.0], egui::Label::new(label).truncate().halign(egui::Align::LEFT));
            add(ui);
        });
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
        ui.allocate_ui_with_layout(egui::vec2(250.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt("sim_graph_kind").selected_text(kind_name(kind)).width(240.0).show_ui(ui, |ui| {
                use spai_ui::widgets::SteadySelect as _;
                for k in GraphKind::ALL {
                    ui.menu_value(&mut sim.graph, k, kind_name(k));
                }
            });
        });
        if damage {
            if kind != GraphKind::DamageVsTargetSignature {
                slider(ui, tr!("Target signature"), &mut |ui| {
                    ui.add(egui::Slider::new(&mut target.signature, 25.0..=2000.0).logarithmic(true).suffix(" m"));
                });
            }
            if kind != GraphKind::DamageVsTargetSpeed {
                slider(ui, tr!("Target speed"), &mut |ui| {
                    ui.add(egui::Slider::new(&mut target.velocity, 0.0..=5000.0).suffix(" m/s"));
                });
                slider(ui, tr!("Transversal"), &mut |ui| {
                    ui.add(egui::Slider::new(&mut target.transversal, 0.0..=5000.0).suffix(" m/s"));
                });
            }
            if kind != GraphKind::DamageVsDistance {
                slider(ui, tr!("Distance"), &mut |ui| {
                    ui.add(egui::Slider::new(&mut target.distance, 0.0..=250_000.0).custom_formatter(|v, _| text::metres(v)));
                });
            }
            ui.allocate_ui_with_layout(egui::vec2(160.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.checkbox(&mut sim.graph_drones, tr!("Include drones"));
            });
        }
    });
    if target != before {
        let t = target.clone();
        sim.edit(EditKind::Target, now_ms, move |f| f.target = t);
    }
    let spec = GraphSpec { include_drones: sim.graph_drones, ..GraphSpec::new(kind, target) };
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
    plot.show(ui, height);
}

/// The graphs in a window of their own, over everything, while `graph_window` is on.
pub fn window(sim: &mut SimUi, ctx: &egui::Context, now_ms: u64) {
    if !sim.graph_window {
        return;
    }
    let mut open = true;
    let size = egui::vec2(760.0, 480.0);
    egui::Window::new(tr!("Graphs")).id(egui::Id::new("sim_graph_window")).open(&mut open).resizable(true).default_size(size).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        show(sim, ui, now_ms);
    });
    sim.graph_window = open;
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
