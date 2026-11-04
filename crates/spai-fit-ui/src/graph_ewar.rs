//! The ewar reaches drawn over the graphs that run against distance: the attackers' own and those
//! of the fits in their Projected tab, each source and kind switched on or off in a popup.

use std::sync::Arc;

use spai_fit::stats::{Computed, RemoteKind};
use spai_ui::{tr, trf};

use crate::plot::{series_color, PlotMarker};
use crate::SimUi;

#[derive(Clone, Debug, PartialEq)]
pub struct Reach {
    /// The fit it is fitted to, by name.
    pub source: String,
    pub kind: RemoteKind,
    pub optimal: f64,
    pub falloff: f64,
}

/// What of a fit's running modules reaches out at an enemy, one reach per kind and range.
pub fn of_fit(source: &str, c: &Computed) -> Vec<Reach> {
    let mut out: Vec<Reach> = Vec::new();
    for m in c.remote.modules.iter().filter(|m| !m.flies && !m.kind.is_assist() && crate::stats_panel::ewar_name(m.kind).is_some()) {
        let r = Reach { source: source.to_owned(), kind: m.kind, optimal: m.optimal, falloff: m.falloff };
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

impl SimUi {
    /// The reaches of each attacker, by session id and name, and of the fits in its Projected tab.
    pub(crate) fn graph_reaches(&mut self, attackers: &[(String, String, Arc<Computed>)]) -> Vec<Reach> {
        let Some(dogma) = self.dogma.clone() else { return Vec::new() };
        let mut out = Vec::new();
        for (id, name, c) in attackers {
            out.extend(of_fit(name, c));
            let Some(fit) = self.sessions().iter().find(|s| s.id == *id).map(|s| s.fit.clone()) else { continue };
            let outside = self.outside();
            for pf in fit.projected_fits.iter().filter(|p| p.active && p.count > 0) {
                let Some(mut src) = outside.fit(&pf.fit_id).cloned() else { continue };
                src.active_variations = pf.variation.filter(|v| src.variations.iter().any(|x| x.id == *v)).into_iter().collect();
                let key = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    spai_fit::doc::to_json(&src).hash(&mut h);
                    (self.profile, self.temp_profile.is_some()).hash(&mut h);
                    h.finish()
                };
                let computed = match self.graph_ewar_cache.get(&key) {
                    Some(c) => c.clone(),
                    None => {
                        let resolved = spai_fit::variations::resolve(&dogma, &src);
                        let c = Arc::new(spai_fit::stats::compute(&dogma, &resolved, self.skills(), &spai_fit::calc::Env::default()));
                        if self.graph_ewar_cache.len() > 64 {
                            self.graph_ewar_cache.clear();
                        }
                        self.graph_ewar_cache.insert(key, c.clone());
                        c
                    }
                };
                for r in of_fit(&src.name, &computed) {
                    if !out.contains(&r) {
                        out.push(r);
                    }
                }
            }
        }
        out
    }
}

fn sources(reaches: &[Reach]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for r in reaches {
        if !v.contains(&r.source) {
            v.push(r.source.clone());
        }
    }
    v
}

/// The reaches not switched off, from nothing out: full strength to the optimal, fading through
/// the falloff. Tackle holds to its optimal only. Coloured by the fit they come from.
pub fn markers(reaches: &[Reach], hidden: &[(String, RemoteKind)]) -> Vec<PlotMarker> {
    let from = sources(reaches);
    let one = from.len() == 1;
    reaches
        .iter()
        .filter(|r| !hidden.iter().any(|(s, k)| *s == r.source && *k == r.kind))
        .filter_map(|r| {
            let name = crate::stats_panel::ewar_name(r.kind)?;
            let i = from.iter().position(|s| *s == r.source).unwrap_or(0);
            let label = if one { name.to_owned() } else { trf!("{ewar} ({fit})", ewar = name, fit = crate::text::clip(&r.source, 20)) };
            Some(PlotMarker { x: r.optimal, label, color: series_color(i + 4).gamma_multiply(0.8), to: (r.falloff > 0.0).then_some(r.optimal + r.falloff), fill: true, bottom: false })
        })
        .collect()
}

pub const POPUP: &str = "sim_graph_ewar_popup";

/// The button over the graph that opens the list of reaches, each fit's kinds a toggle.
pub fn button(ui: &mut egui::Ui, reaches: &[Reach], hidden: &mut Vec<(String, RemoteKind)>) {
    use egui_phosphor::regular as icon;
    use spai_ui::widgets::SteadySelect as _;
    let shown = reaches.iter().filter(|r| !hidden.iter().any(|(s, k)| *s == r.source && *k == r.kind)).count();
    let label = if shown == reaches.len() { trf!("{icon} Ewar", icon = icon::BROADCAST) } else { trf!("{icon} Ewar {shown}/{all}", icon = icon::BROADCAST, shown = shown, all = reaches.len()) };
    let button = ui.button(label).on_hover_text(tr!("Which ewar reaches the graph shows"));
    egui::Popup::from_toggle_button_response(&button).id(egui::Id::new(POPUP)).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(260.0);
        ui.set_max_width(340.0);
        ui.label(egui::RichText::new(tr!("The attackers' ewar and that of the fits in their Projected tab, as reaches over the graph.")).weak());
        ui.horizontal(|ui| {
            if ui.button(tr!("Show all")).clicked() {
                hidden.clear();
            }
            if ui.button(tr!("Hide all")).clicked() {
                *hidden = reaches.iter().map(|r| (r.source.clone(), r.kind)).collect();
            }
        });
        let tip = |on: bool| if on { tr!("Shown on the graph; click to leave it off") } else { tr!("Left off the graph; click to show it") };
        for (i, source) in sources(reaches).into_iter().enumerate() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(crate::text::clip(&source, 40)).strong().color(series_color(i + 4)));
            let mut kinds: Vec<RemoteKind> = Vec::new();
            for r in reaches.iter().filter(|r| r.source == source) {
                if !kinds.contains(&r.kind) {
                    kinds.push(r.kind);
                }
            }
            ui.horizontal_wrapped(|ui| {
                for k in kinds {
                    let on = !hidden.iter().any(|(s, x)| *s == source && *x == k);
                    let name = crate::stats_panel::ewar_name(k).unwrap_or_default();
                    if ui.menu_label(on, name).on_hover_text(tip(on)).clicked() {
                        if on {
                            hidden.push((source.clone(), k));
                        } else {
                            hidden.retain(|(s, x)| !(*s == source && *x == k));
                        }
                    }
                }
            });
        }
    });
}
