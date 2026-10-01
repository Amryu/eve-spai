//! The star map tab: New Eden flat, with the group's holes over it and the Ansiblex network, and a
//! route over gates, bridges and holes. The layers are the desktop's own (`spai_ui::star_map`).

use std::collections::{HashMap, HashSet};

use spai_core::ansiblex::JumpBridge;
use spai_core::geo::Systems;
use spai_core::map::{Bounds, MapSystem};
use spai_core::wormholes::Wormhole;
use spai_ui::star_map::{self as layers, HoleLayer, WhOverlay};

use crate::planner::{PlanInput, RoutePlan};

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
    /// Every system at its true place, for jump ranges.
    pub real: Vec<MapSystem>,
    pub regions: Vec<(i64, String)>,
    pub network: Network,
    pub bridges: HashMap<(i64, i64), (bool, bool)>,
    pub overlay: WhOverlay,
    /// Holes with both ends known, both ways, for routes.
    pub holes: HashMap<i64, Vec<i64>>,
    /// Every live hole, for the systems' tooltips and for routes to pick from.
    pub hole_list: Vec<Wormhole>,
    /// Where each system sits in `real`.
    real_at: HashMap<i64, usize>,
}

impl MapData {
    pub fn new(all: Vec<MapSystem>, real: Vec<MapSystem>, regions: Vec<(i64, String)>) -> Self {
        let real_at = real.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        MapData { all, real, regions, network: Network::default(), bridges: HashMap::new(), overlay: WhOverlay::default(), holes: HashMap::new(), hole_list: Vec::new(), real_at }
    }

    pub fn set_holes(&mut self, holes: &[Wormhole]) {
        self.overlay = WhOverlay::build(holes, |_| true);
        self.hole_list = holes.to_vec();
        self.holes.clear();
        for w in holes {
            if let Some(b) = w.dest_system_id {
                self.holes.entry(w.system_id).or_default().push(b);
                self.holes.entry(b).or_default().push(w.system_id);
            }
        }
    }

    fn real(&self, id: i64) -> Option<&MapSystem> {
        self.real_at.get(&id).map(|&i| &self.real[i])
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
    pub plan: RoutePlan,
    /// The system a right-click opened the menu on.
    menu_sys: Option<i64>,
    pub layers: Layers,
    /// Systems with added characters in them: how many, and whether any is online.
    pub here: HashMap<i64, (usize, bool)>,
}

/// What the map draws over New Eden, as the desktop's layer toggles.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Layers {
    pub wormholes: bool,
    pub jove: bool,
    pub bridges: bool,
    /// Systems tinted by the capital jump range they are in from the system under the pointer.
    pub jump_range: bool,
    /// Systems tinted by the Ansiblex zone they are in around the capital.
    pub zones: bool,
    /// EVE-Scout's Thera and Turnur holes, on both tabs and for routes.
    pub scout: bool,
    /// Holes to Thera and to Turnur drawn, as the desktop's map toggles them.
    pub thera: bool,
    pub turnur: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Layers { wormholes: true, jove: true, bridges: true, jump_range: false, zones: false, scout: true, thera: false, turnur: false }
    }
}

/// The jump range bands, as the desktop colours them: titan, capital, black ops, jump freighter.
const BAND: [egui::Color32; 4] = [
    egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
    egui::Color32::from_rgb(0xE0, 0xA4, 0x3A),
    egui::Color32::from_rgb(0x4F, 0x9B, 0xD8),
    egui::Color32::from_rgb(0xD8, 0x4C, 0x4C),
];

/// Where zones are measured from when the server names no capital.
const CAPITAL: &str = "A24L-V";

const GATE: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xB1, 0x34);
const HOLE: egui::Color32 = egui::Color32::from_rgb(0x4D, 0xD0, 0xC4);
const BRIDGE: egui::Color32 = egui::Color32::from_rgb(0x3A, 0xD0, 0x6A);

impl StarMap {
    pub fn show(&mut self, ui: &mut egui::Ui, geo: &Systems, d: &MapData) {
        if self.zoom <= 0.0 {
            self.zoom = 1.0;
        }
        self.plan.update(&PlanInput { geo, coords: &d.real, holes: &d.hole_list, network: &d.network });
        self.panel(ui, geo, d);
        let draw: Vec<MapSystem> = d.all.iter().filter(|s| self.region.is_none_or(|r| s.region_id == r)).cloned().collect();
        let Some(bounds) = Bounds::of(&draw) else { return };
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let pos: HashMap<i64, egui::Pos2> =
            draw.iter().map(|s| (s.id, spai_core::map::project(s.x, s.z, &bounds, rect, self.zoom, self.pan))).collect();
        let near = |p: egui::Pos2| pos.iter().map(|(id, q)| (*id, q.distance(p))).filter(|(_, dd)| *dd < 10.0).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id);
        let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        let hovered = pointer.and_then(near);
        // A drag that starts on a system draws a route; anywhere else it pans.
        if resp.drag_started_by(egui::PointerButton::Primary) {
            self.plan.link = ui.input(|i| i.pointer.press_origin()).and_then(near);
        }
        if resp.dragged() && self.plan.link.is_none() {
            self.pan += resp.drag_delta();
        }
        if resp.drag_stopped() {
            if let (Some(from), Some(to), Some(at)) = (self.plan.link.take(), hovered, pointer) {
                if from != to {
                    self.plan.link_menu = Some((from, to, at, false));
                }
            }
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let old = self.zoom;
                self.zoom = (old * (scroll * 0.003).exp()).clamp(0.5, 40.0);
                if let Some(m) = pointer {
                    let rel = m - (rect.center() + self.pan);
                    self.pan += rel * (1.0 - self.zoom / old);
                }
            }
        }
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals().clone();
        painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);
        let cull = rect.expand(8.0);
        let dot = (0.5 * self.zoom * if self.region.is_some() { 6.0 } else { 1.0 }).clamp(1.5, 7.0);

        layers::paint_gates(&painter, &visuals, geo, &draw, &pos, &d.bridges, cull);
        let hops = self.plan.route().map(|o| o.hops.clone()).unwrap_or_default();
        // A bridge the route flies is drawn by the route, animated, rather than twice.
        let routed: HashSet<(i64, i64)> =
            hops.windows(2).filter(|w| w[1].kind == 1).map(|w| (w[0].id.min(w[1].id), w[0].id.max(w[1].id))).collect();
        let capital = d.network.capital.clone();
        let no_bridges = HashMap::new();
        let bridges = if self.layers.bridges { &d.bridges } else { &no_bridges };
        layers::paint_bridges(&painter, bridges, &routed, &pos, cull, dot, |a, b| layers::bridge_colors(geo, &capital, a, b, BRIDGE));
        let place = |x: f64, z: f64| spai_core::map::project(x, z, &bounds, rect, self.zoom, self.pan);
        if self.layers.wormholes {
            // The route draws the holes it takes; the plain lines under them are left out.
            let mut taken: HashSet<(i64, i64)> = HashSet::new();
            layers::route_pairs(&hops.iter().map(|h| h.id).collect::<Vec<_>>(), &mut taken);
            layers::paint_wormholes(&painter, &visuals, &d.overlay, &draw, &pos, HoleLayer { turnur: self.layers.turnur, thera: self.layers.thera, spaced: true }, dot, place, &taken);
        }
        if !hops.is_empty() {
            let phase = (ui.input(|i| i.time) * 28.0) as f32;
            let by_hole = |a: i64, b: i64| d.holes.get(&a).is_some_and(|v| v.contains(&b));
            layers::paint_route_legs(&painter, &pos, &hops, phase, HOLE, by_hole, |a, b, fallback| layers::bridge_colors(geo, &capital, a, b, fallback));
            ui.ctx().request_repaint();
        }
        // Near an edge the map pans towards it while a route is dragged, so a far system can be reached.
        if self.plan.link.is_some() {
            if let Some(c) = ui.input(|i| i.pointer.interact_pos()) {
                let content = egui::Rect::from_points(&pos.values().copied().collect::<Vec<_>>());
                let shift = layers::edge_pan(rect, c, content, ui.input(|i| i.stable_dt));
                if shift != egui::Vec2::ZERO {
                    self.pan += shift;
                    ui.ctx().request_repaint();
                }
            }
        }
        // The route drag: a line to the pointer, snapped to the system under it.
        if let (Some(from), Some(p)) = (self.plan.link, pointer) {
            if let Some(a) = pos.get(&from) {
                let b = hovered.and_then(|h| pos.get(&h).copied()).unwrap_or(p);
                painter.line_segment([*a, b], egui::Stroke::new(2.5, GATE));
                painter.circle_stroke(b, 10.0, egui::Stroke::new(2.0, GATE));
                if let Some(to) = hovered.filter(|t| *t != from) {
                    link_tip(ui, p, geo, d, from, to);
                }
            }
        }

        let names = self.region.is_some() || self.zoom >= 6.0;
        let font = egui::FontId::proportional(12.0);
        // Zoomed out the dots touch, and a full halo each would bury them.
        let halo = if dot < 3.0 { dot + 1.5 } else { dot + 4.0 };
        // Under the dots: the zone or jump range band each system is in. The map is flat, so the
        // bands are the dots' colour, not rings, as on the desktop's schematic map.
        if self.layers.zones {
            let cap = if d.network.capital.is_empty() { CAPITAL } else { d.network.capital.as_str() };
            if let Some(c) = geo.lookup(cap).and_then(|i| d.real(i.id)) {
                for s in &draw {
                    let (Some(p), Some(r)) = (pos.get(&s.id), d.real(s.id)) else { continue };
                    if !cull.contains(*p) {
                        continue;
                    }
                    let zone = spai_core::ansiblex::zone_for_ly(spai_core::map::ly_distance(c, r));
                    painter.circle_filled(*p, halo, spai_core::ansiblex::zone_color(zone.clamp(1, 5)).gamma_multiply(0.55));
                }
            }
        } else if self.layers.jump_range {
            if let Some(c) = hovered.or(self.selected).and_then(|h| d.real(h)) {
                for s in &draw {
                    let (Some(p), Some(r)) = (pos.get(&s.id), d.real(s.id)) else { continue };
                    if s.id == c.id || !cull.contains(*p) {
                        continue;
                    }
                    let ly = spai_core::map::ly_distance(c, r);
                    if let Some(b) = spai_core::map::JUMP_RANGES.iter().position(|(_, max)| ly <= *max) {
                        painter.circle_filled(*p, halo, BAND[b.min(3)].gamma_multiply(0.70));
                    }
                }
            }
        }
        let anchors = self.plan.anchors.clone();
        for s in &draw {
            let p = pos[&s.id];
            if !cull.contains(p) {
                continue;
            }
            painter.circle_filled(p, dot, spai_ui::colors::security_color(s.security));
            // The added characters, as the desktop rings its own.
            if let Some((count, online)) = self.here.get(&s.id) {
                let blue = if *online { egui::Color32::from_rgb(0x4F, 0xC3, 0xF7) } else { egui::Color32::from_rgb(0xA8, 0xDE, 0xF7) };
                painter.circle_stroke(p, dot + 8.0, egui::Stroke::new(2.5, blue));
                if *count > 1 {
                    painter.text(p + egui::vec2(dot + 9.0, -(dot + 9.0)), egui::Align2::LEFT_BOTTOM, count.to_string(), egui::FontId::proportional(11.0), blue);
                }
            }
            if anchors.contains(&s.id) {
                painter.circle_stroke(p, dot + 7.0, egui::Stroke::new(2.0, visuals.hyperlink_color));
            }
            if self.selected == Some(s.id) {
                painter.circle_stroke(p, dot + 5.0, egui::Stroke::new(2.5, egui::Color32::WHITE));
            } else if hovered == Some(s.id) {
                painter.circle_stroke(p, dot + 3.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            }
            // Above the dot, centred as one row: the hole mark, the Jove Observatory, then the name.
            // The icons show at any zoom; the name only when it has room.
            let mark = d.overlay.marks.get(&s.id).filter(|_| self.layers.wormholes);
            // Nullsec is full of observatories: marked only once the names show.
            let jove = self.layers.jove && names && spai_core::jove::has(s.id);
            let show_name = names && rect.contains(p);
            if mark.is_some() || jove || show_name {
                let icon_h = (dot * 1.6 + 8.0).clamp(11.0, 20.0);
                let icon_w = icon_h + 3.0;
                let lead = mark.map_or(0.0, layers::hole_mark_slots) + if jove { 1.0 } else { 0.0 };
                let name_g = show_name.then(|| painter.layout_no_wrap(s.name.clone(), font.clone(), visuals.text_color()));
                let name_w = name_g.as_ref().map_or(0.0, |g| g.size().x + if lead > 0.0 { 4.0 } else { 0.0 });
                let mid_y = p.y - dot - 3.0 - icon_h.max(12.0) / 2.0;
                let mut x = p.x - (lead * icon_w + name_w) / 2.0;
                if let Some(m) = mark {
                    layers::paint_hole_mark(&painter, egui::pos2(x, mid_y), icon_h, icon_w, m, d.overlay.jspace_blocked.contains(&s.id));
                    x += layers::hole_mark_slots(m) * icon_w;
                }
                if jove {
                    painter.text(egui::pos2(x, mid_y), egui::Align2::LEFT_CENTER, egui_phosphor::regular::CELL_TOWER, egui::FontId::proportional(icon_h), layers::JOVE_COLOR);
                    x += icon_w;
                }
                if let Some(g) = name_g {
                    let at = egui::pos2(x + if lead > 0.0 { 4.0 } else { 0.0 }, mid_y - g.size().y / 2.0);
                    painter.galley(at, g, visuals.text_color());
                }
            }
        }
        if !names {
            layers::paint_region_labels(&painter, &draw, &pos, &d.regions, rect);
        }
        if resp.clicked() {
            self.selected = hovered;
        }
        if resp.secondary_clicked() {
            self.menu_sys = hovered;
        }
        let menu_sys = self.menu_sys;
        let plan = &mut self.plan;
        resp.context_menu(|ui| {
            // Menus open where the click was; near the right edge egui would wrap every item.
            ui.set_min_width(230.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            let Some(sid) = menu_sys else {
                ui.label(egui::RichText::new("Right-click a system").weak());
                return;
            };
            if let Some(i) = geo.info_of(sid) {
                ui.label(egui::RichText::new(format!("{} \u{b7} {:.1}", i.name, i.security)).strong());
                ui.separator();
            }
            plan.system_menu(ui, sid, geo, &d.hole_list);
        });
        if self.plan.link.is_none() && self.plan.link_menu.is_none() {
            // Beside the pointer: a tooltip of the whole map would sit at the map's corner.
            let menu_open = egui::Popup::is_any_open(ui.ctx());
            if let (Some(h), Some(p), None, false) = (hovered, pointer, ui.ctx().dragged_id(), menu_open) {
                tip_area(ui.ctx(), "web_system_tip", p, |ui| system_tip(ui, geo, d, h));
            }
        }
        self.plan.link_menu(ui);
    }

    fn panel(&mut self, ui: &mut egui::Ui, geo: &Systems, d: &MapData) {
        egui::Panel::left("web_map_panel").resizable(true).default_size(300.0).show_inside(ui, |ui| {
            let region_name = |id: Option<i64>| id.and_then(|r| d.regions.iter().find(|(x, _)| *x == r)).map_or("All of New Eden".to_owned(), |(_, n)| n.clone());
            let mut regions: Vec<&(i64, String)> = d.regions.iter().filter(|(id, n)| *id < 11_000_000 && !n.chars().any(|c| c.is_ascii_digit())).collect();
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
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.layers.wormholes, "Wormholes");
                ui.checkbox(&mut self.layers.bridges, "Ansiblex");
                ui.checkbox(&mut self.layers.jove, "Jove Observatories").on_hover_text("Where drifter holes can lead to; shown once system names show");
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.layers.scout, "EVE-Scout")
                    .on_hover_text("Thera and Turnur holes from EVE-Scout, on both tabs and for routes. They stay in this browser.");
                ui.add_enabled_ui(self.layers.wormholes, |ui| {
                    ui.checkbox(&mut self.layers.thera, format!("{}  Thera", egui_phosphor::regular::PLANET));
                    ui.checkbox(&mut self.layers.turnur, format!("{}  Turnur", egui_phosphor::regular::PLANET));
                });
            });
            // One tint at a time: both colour the same dots.
            ui.horizontal_wrapped(|ui| {
                if ui.checkbox(&mut self.layers.jump_range, format!("{}  Jump range", egui_phosphor::regular::CROSSHAIR_SIMPLE)).on_hover_text("Systems in capital jump range of the one under the pointer, or the selected one").changed() && self.layers.jump_range {
                    self.layers.zones = false;
                }
                if ui.checkbox(&mut self.layers.zones, format!("{}  Ansiblex zones", egui_phosphor::regular::CIRCLES_THREE)).on_hover_text("The zone a bridge landing in each system is priced at, around the capital").changed() && self.layers.zones {
                    self.layers.jump_range = false;
                }
            });
            if self.layers.jump_range {
                legend(ui, spai_core::map::JUMP_RANGES.iter().enumerate().map(|(i, (name, ly))| (BAND[i.min(3)], format!("{name} {ly:.0} ly"))).collect());
            } else if self.layers.zones {
                let cap = if d.network.capital.is_empty() { CAPITAL.to_owned() } else { d.network.capital.clone() };
                let mut rows: Vec<(egui::Color32, String)> =
                    (1..=4u8).map(|z| (spai_core::ansiblex::zone_color(z), format!("Zone {z}: {}\u{2013}{} ly", (z - 1) * 5, z * 5))).collect();
                rows.push((spai_core::ansiblex::zone_color(5), "Zone 5: 20 ly and on".to_owned()));
                ui.label(egui::RichText::new(format!("Around {cap}")).weak());
                legend(ui, rows);
            }
            ui.separator();
            self.plan.panel(ui, geo);
        });
    }
}

/// Colour swatches with what each means.
fn legend(ui: &mut egui::Ui, rows: Vec<(egui::Color32, String)>) {
    for (col, text) in rows {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 5.0, col);
            ui.label(text);
        });
    }
}

/// A tooltip beside the pointer that keeps its width: it opens to the left of the pointer when the
/// right edge is near, rather than squeezing its text into a column.
pub fn tip_area(ctx: &egui::Context, id: &str, at: egui::Pos2, add: impl FnOnce(&mut egui::Ui)) {
    const W: f32 = 340.0;
    let screen = ctx.content_rect();
    let left = at.x + 16.0 + W > screen.right();
    let (pos, pivot) = if left { (at + egui::vec2(-16.0, 16.0), egui::Align2::RIGHT_TOP) } else { (at + egui::vec2(16.0, 16.0), egui::Align2::LEFT_TOP) };
    egui::Area::new(egui::Id::new(id)).order(egui::Order::Tooltip).fixed_pos(pos).pivot(pivot).show(ctx, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        egui::Frame::popup(ui.style()).show(ui, add);
    });
}

/// The readout beside the system a route drag is aimed at: light years, gates, and gates with
/// bridges, which often differ a lot.
fn link_tip(ui: &egui::Ui, at: egui::Pos2, geo: &Systems, d: &MapData, from: i64, to: i64) {
    let find = |id: i64| d.real.iter().find(|s| s.id == id);
    let ly = find(from).zip(find(to)).map(|(a, b)| spai_core::map::ly_distance(a, b));
    let gates = geo.jumps_gates_only(from, to, 200);
    let bridged = geo.jumps(from, to, 200);
    let jumps = |n: Option<u32>| match n {
        Some(1) => "1 jump".to_owned(),
        Some(n) => format!("{n} jumps"),
        None => "no route".to_owned(),
    };
    tip_area(ui.ctx(), "web_link_tip", at, |ui| {
        {
            if let Some(i) = geo.info_of(to) {
                ui.label(egui::RichText::new(&i.name).strong());
            }
            if let Some(ly) = ly {
                ui.label(egui::RichText::new(format!("{ly:.1} ly")).weak());
            }
            ui.label(egui::RichText::new(format!("{} by gate", jumps(gates))).weak());
            if bridged.is_some() && bridged != gates {
                ui.label(egui::RichText::new(format!("{} with bridges", jumps(bridged))).weak());
            }
        }
    });
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
        let d0 = MapData::new(Vec::new(), Vec::new(), Vec::new());
        assert_eq!(find(&geo, &d0, "1DQ1-A", "7-K5EL").unwrap().len(), 3, "two gates");
        assert!(find(&geo, &d0, "1DQ1-A", "Jita").is_err(), "no gate reaches Jita in this universe");
        let mut d = MapData::new(Vec::new(), Vec::new(), Vec::new());
        d.set_holes(&[Wormhole { system_id: 30_004_759, dest_system_id: Some(30_000_142), ..Default::default() }]);
        assert_eq!(find(&geo, &d, "1dq1-a", "jita").unwrap(), vec![30_004_759, 30_000_142]);
        let network = Network { capital: "1DQ1-A".into(), max_zone: 5, bridges: vec![("1DQ1-A".into(), "7-K5EL".into())] };
        spai_core::ansiblex::feed(spai_core::ansiblex::BridgeKey { bridges: network.list(), capital: network.capital.clone(), max_zone: 5 }, &mut geo);
        d.set_network(network, &geo);
        assert_eq!(find(&geo, &d, "1DQ1-A", "7-K5EL").unwrap().len(), 2, "one bridge");
    }
}

/// A system at a glance: where it is, what it is, and the group's holes there. Nothing live: the
/// web map has no ESI kills or jumps.
fn system_tip(ui: &mut egui::Ui, geo: &Systems, d: &MapData, id: i64) {
    let Some(i) = geo.info_of(id) else { return };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&i.name).strong());
        ui.label(egui::RichText::new(format!("{:.1}", (i.security * 10.0).round() / 10.0)).color(spai_ui::colors::security_color(i.security)));
    });
    let place = if i.constellation.is_empty() { i.region.clone() } else { format!("{} \u{b7} {}", i.region, i.constellation) };
    ui.label(egui::RichText::new(place).weak());
    if !i.faction.is_empty() {
        ui.label(egui::RichText::new(&i.faction).weak());
    }
    if spai_core::jove::has(id) {
        ui.label(egui::RichText::new(format!("{}  Jove Observatory", egui_phosphor::regular::CELL_TOWER)).color(layers::JOVE_COLOR));
    }
    let now = spai_core::clock::utc().timestamp();
    let here: Vec<&Wormhole> = d.hole_list.iter().filter(|w| w.system_id == id || w.dest_system_id == Some(id)).collect();
    if !here.is_empty() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("{} hole{}", here.len(), if here.len() == 1 { "" } else { "s" })).strong());
        for w in here.iter().take(8) {
            let near = w.system_id == id;
            let (sig, far) = if near { (&w.signature, w.dest_system_id) } else { (&w.dest_signature, Some(w.system_id)) };
            let far = far.and_then(|f| geo.info_of(f)).map_or_else(|| w.dest.label().to_owned(), |f| f.name.clone());
            let mut facts: Vec<String> = [w.wh_type.clone(), w.mass.map(|m| m.short().to_owned())].into_iter().flatten().collect();
            if let Some(h) = w.hours_left(now) {
                facts.push(format!("{h}h left"));
            }
            let facts = if facts.is_empty() { String::new() } else { format!("  ({})", facts.join(", ")) };
            ui.label(format!("{} {} {far}{facts}", sig.as_deref().unwrap_or("?"), egui_phosphor::regular::ARROW_RIGHT));
            let (added, edited) = spai_ui::wh_graph::who_lines(w, now);
            ui.label(egui::RichText::new(match edited {
                Some(e) => format!("{added} \u{b7} {e}"),
                None => added,
            })
            .weak());
        }
        if here.len() > 8 {
            ui.label(egui::RichText::new(format!("and {} more", here.len() - 8)).weak());
        }
    }
}

