//! The star map tab: New Eden flat, with the group's holes over it and the Ansiblex network, and a
//! route over gates, bridges and holes. The layers are the desktop's own (`spai_ui::star_map`).

use std::collections::{HashMap, HashSet};

use spai_core::ansiblex::JumpBridge;
use spai_core::geo::Systems;
use spai_core::map::{Bounds, MapSystem};
use spai_core::wormholes::Wormhole;
use spai_ui::star_map::{self as layers, HoleLayer, WhOverlay};

/// The Ansiblex network as the server keeps it for the group.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct Network {
    #[serde(default)]
    pub capital: String,
    #[serde(default)]
    pub max_zone: u8,
    #[serde(default)]
    pub bridges: Vec<(String, String)>,
}

impl Network {
    pub fn list(&self) -> Vec<JumpBridge> {
        self.bridges.iter().map(|(a, b)| JumpBridge { from: a.clone(), to: b.clone() }).collect()
    }
}

/// What the map draws, kept between frames: rebuilt when the holes or the network change.
pub struct MapData {
    /// Every k-space system at its place on the flat map.
    pub all: Vec<MapSystem>,
    pub regions: Vec<(i64, String)>,
    pub network: Network,
    pub bridges: HashMap<(i64, i64), (bool, bool)>,
    pub overlay: WhOverlay,
    /// Holes with both ends known, both ways, for routes.
    pub holes: HashMap<i64, Vec<i64>>,
}

impl MapData {
    pub fn new(all: Vec<MapSystem>, regions: Vec<(i64, String)>) -> Self {
        MapData { all, regions, network: Network::default(), bridges: HashMap::new(), overlay: WhOverlay::default(), holes: HashMap::new() }
    }

    pub fn set_holes(&mut self, holes: &[Wormhole]) {
        self.overlay = WhOverlay::build(holes, |_| true);
        self.holes.clear();
        for w in holes {
            if let Some(b) = w.dest_system_id {
                self.holes.entry(w.system_id).or_default().push(b);
                self.holes.entry(b).or_default().push(w.system_id);
            }
        }
    }

    pub fn set_network(&mut self, network: Network, geo: &Systems) {
        self.bridges = layers::bridge_directions(&network.list(), geo, &network.capital, network.max_zone.max(1));
        self.network = network;
    }
}

#[derive(Default)]
pub struct StarMap {
    /// `None` is all of New Eden.
    pub region: Option<i64>,
    zoom: f32,
    pan: egui::Vec2,
    pub selected: Option<i64>,
    from: String,
    to: String,
    route: Option<Result<Vec<i64>, String>>,
}

const GATE: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xB1, 0x34);
const HOLE: egui::Color32 = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
const BRIDGE: egui::Color32 = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);

impl StarMap {
    pub fn show(&mut self, ui: &mut egui::Ui, geo: &Systems, d: &MapData) {
        if self.zoom <= 0.0 {
            self.zoom = 1.0;
        }
        self.panel(ui, geo, d);
        let draw: Vec<MapSystem> = d.all.iter().filter(|s| self.region.is_none_or(|r| s.region_id == r)).cloned().collect();
        let Some(bounds) = Bounds::of(&draw) else { return };
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        if resp.dragged() {
            self.pan += resp.drag_delta();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = self.zoom;
                self.zoom = (old * (scroll * 0.003).exp()).clamp(0.5, 40.0);
                if let Some(m) = ui.input(|i| i.pointer.hover_pos()) {
                    let rel = m - (rect.center() + self.pan);
                    self.pan += rel * (1.0 - self.zoom / old);
                }
            }
        }
        let pos: HashMap<i64, egui::Pos2> =
            draw.iter().map(|s| (s.id, spai_core::map::project(s.x, s.z, &bounds, rect, self.zoom, self.pan))).collect();
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();
        painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);
        let cull = rect.expand(8.0);
        let dot = (0.5 * self.zoom * if self.region.is_some() { 6.0 } else { 1.0 }).clamp(1.5, 7.0);

        layers::paint_gates(&painter, &visuals, geo, &draw, &pos, &d.bridges, cull);
        let route_pairs: HashSet<(i64, i64)> = self.route_legs().into_iter().map(|(a, b)| (a.min(b), a.max(b))).collect();
        let capital = d.network.capital.clone();
        layers::paint_bridges(&painter, &d.bridges, &route_pairs, &pos, cull, dot, |a, b| layers::bridge_colors(geo, &capital, a, b, BRIDGE));
        let place = |x: f64, z: f64| spai_core::map::project(x, z, &bounds, rect, self.zoom, self.pan);
        layers::paint_wormholes(&painter, &visuals, &d.overlay, &draw, &pos, HoleLayer { turnur: true, thera: true, spaced: true }, dot, place);
        let phase = (ui.input(|i| i.time) * 28.0) as f32;
        for (a, b) in self.route_legs() {
            let (Some(pa), Some(pb)) = (pos.get(&a), pos.get(&b)) else { continue };
            if d.bridges.contains_key(&(a.min(b), a.max(b))) {
                let (ca, cb) = layers::bridge_colors(geo, &capital, a, b, BRIDGE);
                layers::polyline_flow_gradient(&painter, &layers::arc_polyline(*pa, *pb, layers::BRIDGE_BOW), ca, cb, phase);
            } else {
                let gate = geo.neighbors_gates_only(a).contains(&b);
                layers::dashed_flow(&painter, *pa, *pb, if gate { GATE } else { HOLE }, phase);
            }
        }
        if self.route.as_ref().is_some_and(|r| r.is_ok()) {
            ui.ctx().request_repaint();
        }

        let hovered = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p)).and_then(|p| {
            pos.iter().map(|(id, q)| (*id, q.distance(p))).filter(|(_, dd)| *dd < 10.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id)
        });
        let names = self.region.is_some() || self.zoom >= 6.0;
        let font = egui::FontId::proportional(12.0);
        for s in &draw {
            let p = pos[&s.id];
            if !cull.contains(p) {
                continue;
            }
            painter.circle_filled(p, dot, spai_ui::colors::security_color(s.security));
            if self.selected == Some(s.id) {
                painter.circle_stroke(p, dot + 5.0, egui::Stroke::new(2.5, egui::Color32::WHITE));
            } else if hovered == Some(s.id) {
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            if names && rect.contains(p) {
                painter.text(p + egui::vec2(0.0, -dot - 3.0), egui::Align2::CENTER_BOTTOM, &s.name, font.clone(), visuals.text_color());
            }
        }
        if !names {
            layers::paint_region_labels(&painter, &draw, &pos, &d.regions, rect);
        }
        if let Some(h) = hovered {
            if let Some(i) = geo.info_of(h) {
                resp.clone().on_hover_text(format!("{} \u{b7} {:.1} \u{b7} {}", i.name, i.security, i.region));
            }
        }
        if resp.clicked() {
            self.selected = hovered;
        }
    }

    fn route_legs(&self) -> Vec<(i64, i64)> {
        match &self.route {
            Some(Ok(path)) => path.windows(2).map(|w| (w[0], w[1])).collect(),
            _ => Vec::new(),
        }
    }

    fn panel(&mut self, ui: &mut egui::Ui, geo: &Systems, d: &MapData) {
        egui::Panel::left("web_map_panel").resizable(true).default_size(260.0).show_inside(ui, |ui| {
            let region_name = |id: Option<i64>| id.and_then(|r| d.regions.iter().find(|(x, _)| *x == r)).map_or("All of New Eden".to_owned(), |(_, n)| n.clone());
            let mut regions: Vec<&(i64, String)> = d.regions.iter().filter(|(id, _)| *id < 11_000_000).collect();
            regions.sort_by(|a, b| a.1.cmp(&b.1));
            let before = self.region;
            egui::ComboBox::from_id_salt("web_map_region").width(ui.available_width()).selected_text(region_name(self.region)).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.region, None, "All of New Eden");
                for (id, name) in regions {
                    ui.selectable_value(&mut self.region, Some(*id), name);
                }
            });
            if self.region != before {
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;
            }
            ui.add_space(8.0);
            ui.strong("Route");
            egui::Grid::new("web_route").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("From");
                ui.add(egui::TextEdit::singleline(&mut self.from).hint_text("System").desired_width(160.0));
                ui.end_row();
                ui.label("To");
                ui.add(egui::TextEdit::singleline(&mut self.to).hint_text("System").desired_width(160.0));
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button("Find route").clicked() {
                    self.route = Some(find(geo, d, &self.from, &self.to));
                }
                if let Some(sel) = self.selected.and_then(|s| geo.info_of(s)) {
                    if ui.small_button(format!("To {}", sel.name)).clicked() {
                        self.to = sel.name.clone();
                    }
                }
            });
            match &self.route {
                Some(Ok(path)) => {
                    let jumps = path.len().saturating_sub(1);
                    ui.label(format!("{jumps} jump{}", if jumps == 1 { "" } else { "s" }));
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for w in path.windows(2) {
                            let kind = if d.bridges.contains_key(&(w[0].min(w[1]), w[0].max(w[1]))) {
                                ("Ansiblex", BRIDGE)
                            } else if geo.neighbors_gates_only(w[0]).contains(&w[1]) {
                                ("gate", GATE)
                            } else {
                                ("wormhole", HOLE)
                            };
                            let name = geo.info_of(w[1]).map_or_else(|| w[1].to_string(), |i| i.name.clone());
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(kind.0).color(kind.1));
                                ui.label(name);
                            });
                        }
                    });
                }
                Some(Err(e)) => {
                    ui.label(egui::RichText::new(e).color(ui.visuals().error_fg_color));
                }
                None => {
                    ui.label(egui::RichText::new("Over gates, the group's Ansiblex bridges and its known holes.").weak());
                }
            }
        });
    }
}

/// The shortest way between two systems named by the user.
pub fn find(geo: &Systems, d: &MapData, from: &str, to: &str) -> Result<Vec<i64>, String> {
    let sys = |n: &str| geo.lookup(n.trim()).or_else(|| geo.lookup_prefix(n.trim())).map(|i| i.id).ok_or_else(|| format!("No system called {:?}", n.trim()));
    let (a, b) = (sys(from)?, sys(to)?);
    geo.route_with(a, b, true, true, &d.holes, |_| true).ok_or_else(|| "No way there over gates, bridges or known holes".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_takes_a_known_hole_when_it_is_shorter() {
        let mut geo = spai_core::test_support::small_universe(&[]);
        let d0 = MapData::new(Vec::new(), Vec::new());
        assert_eq!(find(&geo, &d0, "1DQ1-A", "7-K5EL").unwrap().len(), 3, "two gates");
        assert!(find(&geo, &d0, "1DQ1-A", "Jita").is_err(), "no gate reaches Jita in this universe");
        let mut d = MapData::new(Vec::new(), Vec::new());
        d.set_holes(&[Wormhole { system_id: 30_004_759, dest_system_id: Some(30_000_142), ..Default::default() }]);
        assert_eq!(find(&geo, &d, "1dq1-a", "jita").unwrap(), vec![30_004_759, 30_000_142]);
        let network = Network { capital: "1DQ1-A".into(), max_zone: 5, bridges: vec![("1DQ1-A".into(), "7-K5EL".into())] };
        spai_core::ansiblex::feed(spai_core::ansiblex::BridgeKey { bridges: network.list(), capital: network.capital.clone(), max_zone: 5 }, &mut geo);
        d.set_network(network, &geo);
        assert_eq!(find(&geo, &d, "1DQ1-A", "7-K5EL").unwrap().len(), 2, "one bridge");
    }
}
