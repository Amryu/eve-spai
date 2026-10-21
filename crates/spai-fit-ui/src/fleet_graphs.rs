//! The fleet's graphs: both fleets' damage over range with where each is favoured and where the
//! ewar reaches, the race over time, and the angular velocities a spread fleet's guns must track.

use spai_fit::fleet::sweep::{self, Side};
use spai_ui::{tr, trf};

use crate::fleet_panel::Analysis;
use crate::plot::{series_color, Plot, PlotBand, PlotMarker, PlotSeries};
use crate::{text, SimUi};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FleetGraph {
    #[default]
    DamageByRange,
    VolleyByRange,
    ShipsOverTime,
    DamageOverTime,
    Angular,
}

impl FleetGraph {
    pub const ALL: [FleetGraph; 5] = [FleetGraph::DamageByRange, FleetGraph::VolleyByRange, FleetGraph::ShipsOverTime, FleetGraph::DamageOverTime, FleetGraph::Angular];

    pub fn name(self) -> &'static str {
        match self {
            FleetGraph::DamageByRange => tr!("Damage by range"),
            FleetGraph::VolleyByRange => tr!("Volley by range"),
            FleetGraph::ShipsOverTime => tr!("Ships left in the race"),
            FleetGraph::DamageOverTime => tr!("Damage landing in the race"),
            FleetGraph::Angular => tr!("Angular velocity across the formations"),
        }
    }
}

const GOOD: egui::Color32 = egui::Color32::from_rgb(92, 184, 92);
const BAD: egui::Color32 = egui::Color32::from_rgb(214, 96, 96);

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui) {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| body(sim, ui));
}

fn body(sim: &mut SimUi, ui: &mut egui::Ui) {
    let width = ui.available_width();
    ui.horizontal(|ui| {
        use spai_ui::widgets::SteadySelect as _;
        ui.label(tr!("Plot"));
        egui::ComboBox::from_id_salt("sim_fleet_graph").selected_text(sim.fleet_graph.name()).width(280.0f32.min(width - 60.0)).show_ui(ui, |ui| {
            for g in FleetGraph::ALL {
                if ui.menu_label(sim.fleet_graph == g, g.name()).clicked() {
                    sim.fleet_graph = g;
                }
            }
        });
    });
    ui.add_space(4.0);
    let Some(an) = sim.fleet_analysis() else {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(if sim.fleet.is_none() { tr!("No fleet open.") } else { tr!("Pick a fleet under Against to compare.") }).weak());
        return;
    };
    let plot = match sim.fleet_graph {
        FleetGraph::DamageByRange => by_range(ui, &an),
        FleetGraph::VolleyByRange => volley_by_range(ui, &an),
        FleetGraph::ShipsOverTime => over_time(&an, true),
        FleetGraph::DamageOverTime => over_time(&an, false),
        FleetGraph::Angular => angular(&an),
    };
    let height = (ui.available_height() - 4.0).max(200.0);
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_max_width(width);
        plot.show(ui, height);
    });
}

fn plain() -> Plot {
    Plot { x_label: String::new(), y_label: String::new(), series: Vec::new(), fmt_x: text::num, fmt_y: text::num, y_to: None, bands: Vec::new(), markers: Vec::new() }
}

/// Damage each fleet lands on the other's primary over range, shaded where either is favoured,
/// with the best ranges, the set range and where each side's ewar stops.
fn by_range(ui: &egui::Ui, an: &Analysis) -> Plot {
    let sw = &an.sweep;
    let (a, b) = (series_color(0), series_color(1));
    let mut markers = vec![
        PlotMarker { x: f64::from(an.range), label: tr!("set range").to_owned(), color: ui.visuals().text_color() },
        PlotMarker { x: sw.a_best, label: trf!("{a} best", a = text::clip(&an.mine.name, 18)), color: a },
        PlotMarker { x: sw.b_best, label: trf!("{a} best", a = text::clip(&an.theirs.name, 18)), color: b },
    ];
    for (cuts, color) in [(&sw.a_cutoffs, a), (&sw.b_cutoffs, b)] {
        for (kind, optimal, _) in cuts {
            if let Some(name) = crate::stats_panel::ewar_name(*kind) {
                markers.push(PlotMarker { x: *optimal, label: name.to_owned(), color: color.gamma_multiply(0.8) });
            }
        }
    }
    let bands = sw.a_favoured.iter().map(|(f, t)| PlotBand { from: *f, to: *t, color: GOOD }).chain(sw.b_favoured.iter().map(|(f, t)| PlotBand { from: *f, to: *t, color: BAD })).collect();
    Plot {
        x_label: tr!("Distance").to_owned(),
        y_label: tr!("DPS on the primary").to_owned(),
        fmt_x: text::metres,
        series: vec![
            PlotSeries { label: an.mine.name.clone(), color: a, points: sw.points.iter().map(|p| [p.distance, p.a.applied_dps]).collect() },
            PlotSeries { label: an.theirs.name.clone(), color: b, points: sw.points.iter().map(|p| [p.distance, p.b.applied_dps]).collect() },
        ],
        bands,
        markers,
        ..plain()
    }
}

/// One volley of each fleet on the other's first target over range, with that target's effective
/// hit points flat across: above it, one volley kills.
fn volley_by_range(ui: &egui::Ui, an: &Analysis) -> Plot {
    let sw = &an.sweep;
    let (a, b) = (series_color(0), series_color(1));
    let mut series = vec![
        PlotSeries { label: an.mine.name.clone(), color: a, points: sw.points.iter().map(|p| [p.distance, p.a.applied_volley]).collect() },
        PlotSeries { label: an.theirs.name.clone(), color: b, points: sw.points.iter().map(|p| [p.distance, p.b.applied_volley]).collect() },
    ];
    let far = sw.points.last().map_or(1.0, |p| p.distance);
    for (fleet, other, color) in [(&an.fleet, &an.theirs, a), (&an.enemy, &an.mine, b)] {
        if let Some(t) = sweep::primary(fleet, other) {
            let ehp = t.computed.tank.ehp;
            series.push(PlotSeries { label: trf!("{a} effective HP", a = text::clip(&t.name, 22)), color: color.gamma_multiply(0.55), points: vec![[0.0, ehp], [far, ehp]] });
        }
    }
    Plot {
        x_label: tr!("Distance").to_owned(),
        y_label: tr!("Volley on the primary").to_owned(),
        fmt_x: text::metres,
        series,
        markers: vec![PlotMarker { x: f64::from(an.range), label: tr!("set range").to_owned(), color: ui.visuals().text_color() }],
        ..plain()
    }
}

/// The race at the set range: ships left on each side, or the damage each lands, over time, with
/// where each loses its critical mass.
fn over_time(an: &Analysis, ships: bool) -> Plot {
    let r = &an.race;
    let pick = |x: &(f64, u32, u32, f64, f64), side: usize| match (ships, side) {
        (true, 0) => f64::from(x.1),
        (true, _) => f64::from(x.2),
        (false, 0) => x.3,
        (false, _) => x.4,
    };
    let names = [&an.mine.name, &an.theirs.name];
    let series = (0..2).map(|side| PlotSeries { label: names[side].clone(), color: series_color(side), points: r.timeline.iter().map(|x| [x.0, pick(x, side)]).collect() }).collect();
    let markers = (0..2).filter_map(|side| r.critical_mass_lost[side].map(|t| PlotMarker { x: t, label: trf!("{a} loses critical mass", a = text::clip(names[side], 18)), color: series_color(side) })).collect();
    Plot {
        x_label: tr!("Time").to_owned(),
        y_label: if ships { tr!("Ships") } else { tr!("DPS on the primary") }.to_owned(),
        fmt_x: |v| text::secs(v),
        series,
        markers,
        ..plain()
    }
}

/// Every shooter-target pair's angular velocity at the set range, sorted: the share of pairs at or
/// below a value. Flat lines mark where each side's turrets still hit the other's primary half
/// the time.
fn angular(an: &Analysis) -> Plot {
    let a = Side { fleet: &an.fleet, stats: &an.mine, ammo: &an.ammo[0] };
    let b = Side { fleet: &an.enemy, stats: &an.theirs, ammo: &an.ammo[1] };
    let d = f64::from(an.range);
    let share = |v: Vec<f64>| -> Vec<[f64; 2]> {
        let n = v.len().max(1) as f64;
        v.into_iter().enumerate().map(|(i, x)| [100.0 * (i as f64 + 0.5) / n, x]).collect()
    };
    let mut series = vec![
        PlotSeries { label: trf!("{a} on {b}", a = text::clip(&an.mine.name, 18), b = text::clip(&an.theirs.name, 18)), color: series_color(0), points: share(sweep::angular(&a, &b, d)) },
        PlotSeries { label: trf!("{a} on {b}", a = text::clip(&an.theirs.name, 18), b = text::clip(&an.mine.name, 18)), color: series_color(1), points: share(sweep::angular(&b, &a, d)) },
    ];
    for (side, (own, other, stats)) in [(&a, &b, &an.mine), (&b, &a, &an.theirs)].into_iter().enumerate() {
        let Some(target) = sweep::primary(own.fleet, other.stats) else { continue };
        let mut seen = Vec::new();
        for (k, w, rad) in sweep::half_hit_angular(own, other, target, d) {
            if seen.contains(&(k, w.type_id)) {
                continue;
            }
            seen.push((k, w.type_id));
            let name = stats.slots.get(k).map_or(String::new(), |s| s.name.clone());
            series.push(PlotSeries { label: trf!("{a} hits half at", a = text::clip(&name, 22)), color: series_color(side).gamma_multiply(0.55), points: vec![[0.0, rad], [100.0, rad]] });
        }
    }
    Plot {
        x_label: tr!("Share of shooter and target pairs").to_owned(),
        y_label: tr!("Angular velocity (rad/s)").to_owned(),
        fmt_x: |v| format!("{v:.0}%"),
        fmt_y: |v| format!("{v:.3}"),
        series,
        ..plain()
    }
}
