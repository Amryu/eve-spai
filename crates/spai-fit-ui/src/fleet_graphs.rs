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
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| body(sim, ui, None));
}

/// The buttons that move the graphs: to the left, below or right of the fleet, into a window of
/// their own, or shut.
fn place_buttons(sim: &mut SimUi, ui: &mut egui::Ui, close: bool) {
    use crate::GraphPlace;
    use egui_phosphor::regular as icon;
    use spai_ui::widgets::icon_button;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if close && icon_button(ui, icon::X).on_hover_text(tr!("Close the graphs")).clicked() {
            sim.fleet_graph_open = false;
        }
        for (place, glyph, hint) in [
            (GraphPlace::Window, icon::ARROW_SQUARE_OUT, tr!("Graphs in their own window")),
            (GraphPlace::Right, icon::ARROW_LINE_RIGHT, tr!("Dock the graphs on the right")),
            (GraphPlace::Bottom, icon::ARROW_LINE_DOWN, tr!("Dock the graphs below")),
            (GraphPlace::Left, icon::ARROW_LINE_LEFT, tr!("Dock the graphs on the left")),
        ] {
            if sim.fleet_graph_place != place && icon_button(ui, glyph).on_hover_text(hint).clicked() {
                sim.fleet_graph_place = place;
                sim.fleet_graph_size = None;
            }
        }
    });
}

/// The graphs docked where they were put, with a bar to move or close them.
pub fn docked(sim: &mut SimUi, ui: &mut egui::Ui) {
    use crate::{DockSide, GraphPlace};
    let side = match sim.fleet_graph_place {
        GraphPlace::Window => return,
        GraphPlace::Left => DockSide::Left,
        GraphPlace::Right => DockSide::Right,
        GraphPlace::Bottom => DockSide::Bottom,
    };
    let avail = ui.available_size();
    let (default, min, max) = if side == DockSide::Bottom { (avail.y * 0.4, 220.0, (avail.y - 200.0).max(240.0)) } else { (avail.x * 0.4, 360.0, (avail.x - 320.0).max(360.0)) };
    // A panel of its own per side: a size kept for one is no size for another.
    let id = egui::Id::new(("sim_fleet_graph_dock", side as u8));
    let mut size = sim.fleet_graph_size;
    crate::dock_panel(ui, id, side, &mut size, default, min..=max, |ui| body(sim, ui, Some(true)));
    sim.fleet_graph_size = size;
}

/// The graphs in a window of their own, when put there.
pub fn window(sim: &mut SimUi, ctx: &egui::Context) {
    if !sim.fleet_graph_open || sim.fleet_graph_place != crate::GraphPlace::Window {
        return;
    }
    let size = egui::vec2(900.0, 560.0);
    let mut open = true;
    egui::Window::new(tr!("Fleet graphs")).id(egui::Id::new("sim_fleet_graph_window")).open(&mut open).resizable(true).default_size(size).min_size(egui::vec2(480.0, 320.0)).default_pos(ctx.content_rect().center() - size / 2.0).show(ctx, |ui| {
        // As big as it was dragged, not as its content: a minimum, which adds no spacing of
        // its own the way filling what is left after the content does, frame after frame.
        ui.set_min_size(ui.available_size());
        body(sim, ui, Some(false));
    });
    if !open {
        sim.fleet_graph_open = false;
    }
}

/// The graph; `place`, when docked or in the window, puts the buttons that move it on the plot's
/// line, with a close button of its own when docked.
fn body(sim: &mut SimUi, ui: &mut egui::Ui, place: Option<bool>) {
    let _t = spai_ui::timing::span("sim: fleet graphs");
    let width = ui.available_width();
    ui.horizontal(|ui| {
        use spai_ui::widgets::SteadySelect as _;
        ui.label(tr!("Plot"));
        let buttons_w = if place.is_some() { 5.0 * 28.0 } else { 0.0 };
        crate::combo::combo(ui, "sim_fleet_graph", 280.0f32.min(width - 60.0 - buttons_w).max(120.0), sim.fleet_graph.name(), |ui| {
            for g in FleetGraph::ALL {
                if ui.menu_label(sim.fleet_graph == g, g.name()).clicked() {
                    sim.fleet_graph = g;
                }
            }
        });
        if let Some(close) = place {
            place_buttons(sim, ui, close);
        }
    });
    ui.add_space(4.0);
    let analysis = sim.fleet_analysis();
    if sim.fleet_analysis_pending() {
        crate::fleet_panel::working_badge(ui, sim.fleet_analysis_progress());
    }
    let Some(an) = analysis else {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(if sim.fleet.is_none() { tr!("No fleet open.") } else { tr!("Pick a fleet under Against to compare.") }).weak());
        return;
    };
    if matches!(sim.fleet_graph, FleetGraph::DamageByRange | FleetGraph::VolleyByRange) {
        ewar_filter(ui, &an, &mut sim.fleet_graph_hidden_ewar);
    }
    let hidden = sim.fleet_graph_hidden_ewar.clone();
    let dogma = sim.dogma.clone();
    let plot = match sim.fleet_graph {
        FleetGraph::DamageByRange => by_range(ui, &an, &hidden, dogma.as_deref()),
        FleetGraph::VolleyByRange => volley_by_range(ui, &an, &hidden),
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
    Plot { x_label: String::new(), y_label: String::new(), series: Vec::new(), fmt_x: text::num, fmt_y: text::num, y_to: None, bands: Vec::new(), markers: Vec::new(), levels: Vec::new() }
}

/// The ewar kinds either side brings, by name, each a toggle for the graph.
fn ewar_filter(ui: &mut egui::Ui, an: &Analysis, hidden: &mut Vec<spai_fit::stats::RemoteKind>) {
    use spai_ui::widgets::SteadySelect as _;
    let mut kinds: Vec<spai_fit::stats::RemoteKind> = Vec::new();
    for (k, _, _) in an.sweep.a_cutoffs.iter().chain(&an.sweep.b_cutoffs) {
        if crate::stats_panel::ewar_name(*k).is_some() && !kinds.contains(k) {
            kinds.push(*k);
        }
    }
    if kinds.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tr!("Ewar")).weak());
        for k in kinds {
            let on = !hidden.contains(&k);
            let name = crate::stats_panel::ewar_name(k).unwrap_or_default();
            if ui.menu_label(on, name).on_hover_text(if on { tr!("Shown on the graph; click to leave it off") } else { tr!("Left off the graph; click to show it") }).clicked() {
                if on {
                    hidden.push(k);
                } else {
                    hidden.retain(|x| *x != k);
                }
            }
        }
    });
    ui.add_space(2.0);
}

/// Each side's ewar as reaches from nothing out: full strength to its optimal, then fading
/// through its falloff. Tackle holds to its optimal only.
fn ewar_markers(an: &Analysis, hidden: &[spai_fit::stats::RemoteKind]) -> Vec<PlotMarker> {
    let sw = &an.sweep;
    let mut out = Vec::new();
    for (cuts, color) in [(&sw.a_cutoffs, series_color(0)), (&sw.b_cutoffs, series_color(1))] {
        for (kind, optimal, edge) in cuts.iter().filter(|(k, _, _)| !hidden.contains(k)) {
            if let Some(name) = crate::stats_panel::ewar_name(*kind) {
                out.push(PlotMarker { x: *optimal, label: name.to_owned(), color: color.gamma_multiply(0.8), to: (*edge > *optimal).then_some(*edge), fill: true, bottom: false });
            }
        }
    }
    out
}

/// The charge each side's primary weapon, the type doing most of its damage, starts on and where
/// it switches to another carried charge as the range grows, labelled along the bottom.
fn ammo_markers(an: &Analysis, dogma: Option<&spai_fit::Dogma>) -> Vec<PlotMarker> {
    let Some(dogma) = dogma else { return Vec::new() };
    let mut out = Vec::new();
    for (side, stats) in [&an.mine, &an.theirs].into_iter().enumerate() {
        let mut by_type: Vec<(spai_fit::ids::TypeId, f64)> = Vec::new();
        for s in stats.slots.iter().filter(|s| s.count > 0) {
            for w in &s.computed.weapons {
                let dps = w.dps * f64::from(s.count);
                match by_type.iter_mut().find(|(t, _)| *t == w.type_id) {
                    Some((_, d)) => *d += dps,
                    None => by_type.push((w.type_id, dps)),
                }
            }
        }
        let Some((weapon, _)) = by_type.into_iter().max_by(|x, y| x.1.total_cmp(&y.1)) else { continue };
        // The slot flying most of it, whose charges stand for the fleet's.
        let slot = stats.slots.iter().enumerate().filter(|(_, s)| s.count > 0).max_by(|(_, x), (_, y)| {
            let of = |s: &spai_fit::fleet::SlotStats| s.computed.weapons.iter().filter(|w| w.type_id == weapon).map(|w| w.dps * f64::from(s.count)).sum::<f64>();
            of(x).total_cmp(&of(y))
        });
        let Some((k, _)) = slot else { continue };
        let Some((_, _, bands)) = an.ammo_bands[side].iter().find(|(s, w, _)| *s == k && *w == weapon) else { continue };
        // The charge it starts on, then each switch.
        for (i, band) in bands.iter().enumerate() {
            if i == 0 || band.charge != bands[i - 1].charge {
                out.push(PlotMarker { x: band.from, label: spai_fit::formats::name_of(dogma, band.charge), color: series_color(side).gamma_multiply(0.9), to: None, fill: false, bottom: true });
            }
        }
    }
    out
}

/// Damage each fleet lands on the other's primary over range, shaded where either is favoured,
/// with the best ranges, the set range, each side's ewar reach and where its primary weapon
/// switches charges.
fn by_range(ui: &egui::Ui, an: &Analysis, hidden: &[spai_fit::stats::RemoteKind], dogma: Option<&spai_fit::Dogma>) -> Plot {
    let sw = &an.sweep;
    let (a, b) = (series_color(0), series_color(1));
    let mut markers = vec![
        PlotMarker { x: f64::from(an.range), label: tr!("set range").to_owned(), color: ui.visuals().text_color(), to: None, fill: false, bottom: false },
        PlotMarker { x: sw.a_best, label: trf!("{a} best", a = text::clip(&an.mine.name, 18)), color: a, to: None, fill: false, bottom: false },
        PlotMarker { x: sw.b_best, label: trf!("{a} best", a = text::clip(&an.theirs.name, 18)), color: b, to: None, fill: false, bottom: false },
    ];
    markers.extend(ewar_markers(an, hidden));
    markers.extend(ammo_markers(an, dogma));
    let bands = sw.a_favoured.iter().map(|(f, t)| PlotBand { from: *f, to: *t, color: GOOD }).chain(sw.b_favoured.iter().map(|(f, t)| PlotBand { from: *f, to: *t, color: BAD })).collect();
    Plot {
        x_label: tr!("Distance").to_owned(),
        y_label: tr!("Effective DPS on the primary").to_owned(),
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
fn volley_by_range(ui: &egui::Ui, an: &Analysis, hidden: &[spai_fit::stats::RemoteKind]) -> Plot {
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
        y_label: tr!("Effective volley on the primary").to_owned(),
        fmt_x: text::metres,
        series,
        markers: std::iter::once(PlotMarker { x: f64::from(an.range), label: tr!("set range").to_owned(), color: ui.visuals().text_color(), to: None, fill: false, bottom: false }).chain(ewar_markers(an, hidden)).collect(),
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
    let markers = (0..2).filter_map(|side| r.critical_mass_lost[side].map(|t| PlotMarker { x: t, label: trf!("{a} loses critical mass", a = text::clip(names[side], 18)), color: series_color(side), to: None, fill: false, bottom: false })).collect();
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
