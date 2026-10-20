//! The graphs tab: which curve, what target, and the chart.

use spai_fit::graphs::{self, GraphKind, GraphSpec, SeriesLabel};
use spai_fit::history::EditKind;
use spai_ui::widgets::SteadySelect as _;
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
    }
}

fn axes(kind: GraphKind) -> (&'static str, &'static str, fn(f64) -> String, fn(f64) -> String) {
    fn km(v: f64) -> String {
        format!("{:.0} km", v / 1000.0)
    }
    fn plain(v: f64) -> String {
        text::num(v)
    }
    fn s(v: f64) -> String {
        format!("{v:.0} s")
    }
    fn au(v: f64) -> String {
        format!("{v:.0} AU")
    }
    match kind {
        GraphKind::DamageVsDistance => (tr!("Distance"), tr!("DPS"), km, plain),
        GraphKind::DamageVsTargetSpeed => (tr!("Target speed (m/s)"), tr!("DPS"), plain, plain),
        GraphKind::DamageVsTargetSignature => (tr!("Target signature (m)"), tr!("DPS"), plain, plain),
        GraphKind::CapacitorOverTime => (tr!("Time"), tr!("GJ"), s, plain),
        GraphKind::ShieldOverTime => (tr!("Time"), tr!("Shield HP"), s, plain),
        GraphKind::VelocityOverTime => (tr!("Time"), tr!("m/s"), s, plain),
        GraphKind::WarpTime => (tr!("Distance"), tr!("Seconds"), au, plain),
        GraphKind::LockTime => (tr!("Target signature (m)"), tr!("Seconds"), plain, plain),
    }
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui, now_ms: u64) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(computed) = sim.computed() else {
        ui.label(egui::RichText::new(tr!("No fit open.")).weak());
        return;
    };
    let kind = sim.graph;
    egui::ComboBox::from_id_salt("sim_graph_kind").selected_text(kind_name(kind)).width(ui.available_width().min(260.0)).show_ui(ui, |ui| {
        for k in GraphKind::ALL {
            ui.menu_value(&mut sim.graph, k, kind_name(k));
        }
    });
    let mut target = sim.session.as_ref().map(|s| s.fit.target.clone()).unwrap_or_default();
    let damage = matches!(kind, GraphKind::DamageVsDistance | GraphKind::DamageVsTargetSpeed | GraphKind::DamageVsTargetSignature);
    if damage {
        let before = target.clone();
        egui::Grid::new("sim_graph_target").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            if kind != GraphKind::DamageVsTargetSignature {
                ui.label(tr!("Target signature"));
                ui.add(egui::Slider::new(&mut target.signature, 25.0..=2000.0).logarithmic(true).suffix(" m"));
                ui.end_row();
            }
            if kind != GraphKind::DamageVsTargetSpeed {
                ui.label(tr!("Target speed"));
                ui.add(egui::Slider::new(&mut target.velocity, 0.0..=5000.0).suffix(" m/s"));
                ui.end_row();
                ui.label(tr!("Transversal"));
                ui.add(egui::Slider::new(&mut target.transversal, 0.0..=5000.0).suffix(" m/s"));
                ui.end_row();
            }
            if kind != GraphKind::DamageVsDistance {
                ui.label(tr!("Distance"));
                ui.add(egui::Slider::new(&mut target.distance, 0.0..=250_000.0).logarithmic(true).custom_formatter(|v, _| text::metres(v)));
                ui.end_row();
            }
        });
        if target != before {
            let t = target.clone();
            sim.edit(EditKind::Target, now_ms, move |f| f.target = t);
        }
        ui.checkbox(&mut sim.graph_drones, tr!("Include drones"));
    }
    let spec = GraphSpec { include_drones: sim.graph_drones, ..GraphSpec::new(kind, target) };
    let cap = sim.cap_inputs();
    let series = graphs::sample(&computed, &spec, cap.as_ref());
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
                },
                color: if s.label == SeriesLabel::Total { ui.visuals().text_color() } else { series_color(i) },
                points: s.points.clone(),
            })
            .collect(),
    };
    let height = (ui.available_height() - 8.0).clamp(220.0, 420.0);
    plot.show(ui, height);
}
