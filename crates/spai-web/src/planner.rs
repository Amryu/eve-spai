//! Routes planned on the web map, the desktop's way: drag from a system to another and pick gate,
//! jump or titan; right-click a system to start a route there, set a destination, add a waypoint
//! or avoid it; pick alternatives per leg and branches at forks. The planning is the shared
//! `spai_core::route`, so both maps answer alike.

use std::collections::{HashMap, HashSet};

use egui_phosphor::regular as icon;
use spai_core::geo::Systems;
use spai_core::jumproute::{max_range_ly, SHIP_CLASSES};
use spai_core::map::MapSystem;
use spai_core::route::{self, Avoid, LegChoice, Picks, RouteOption};
use spai_ui::widgets::SteadySelect as _;

/// A titan at JDC V, as the desktop plans with.
const TITAN_LY: f64 = 6.0;

/// What the planner keeps between visits: the avoid lists and the jump ship.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanPrefs {
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    pub ship: usize,
    pub jdc: u32,
    pub jfc: u32,
    pub via_holes: bool,
}

impl Default for PlanPrefs {
    fn default() -> Self {
        PlanPrefs { avoid_gate: Vec::new(), avoid_jump: Vec::new(), ship: 0, jdc: 5, jfc: 5, via_holes: true }
    }
}

#[derive(Default)]
pub struct RoutePlan {
    /// "gate", "jump" or "titan".
    pub kind: &'static str,
    /// Start, waypoints, destination.
    pub anchors: Vec<i64>,
    pub opts: Vec<RouteOption>,
    pub legs: Vec<LegChoice>,
    leg_pick: Vec<usize>,
    forks: Picks,
    avoid_once: HashSet<i64>,
    titans: Vec<i64>,
    titan_at_start: bool,
    titan_self_jump: bool,
    pub prefs: PlanPrefs,
    /// The prefs changed and want saving.
    pub dirty: bool,
    /// A drag started on this system.
    pub link: Option<i64>,
    /// A drag let go on a system: where from, where to, and where on screen, for the kind menu.
    pub link_menu: Option<(i64, i64, egui::Pos2)>,
    /// The anchors changed since the route was worked out.
    stale: bool,
}

/// What a route needs from the map to be worked out.
pub struct PlanInput<'a> {
    pub geo: &'a Systems,
    /// Every system with its true position, for jump ranges.
    pub coords: &'a [MapSystem],
    pub holes: &'a HashMap<i64, Vec<i64>>,
}

impl RoutePlan {
    pub fn active(&self) -> bool {
        !self.anchors.is_empty()
    }

    pub fn route(&self) -> Option<&RouteOption> {
        self.opts.first()
    }

    fn avoid(&self) -> Avoid {
        let always = if self.kind == "jump" { &self.prefs.avoid_jump } else { &self.prefs.avoid_gate };
        Avoid { always: always.iter().copied().collect(), once: self.avoid_once.clone() }
    }

    pub fn start(&mut self, kind: &'static str, sid: i64) {
        *self = RoutePlan { kind, anchors: vec![sid], prefs: std::mem::take(&mut self.prefs), dirty: self.dirty, ..Default::default() };
    }

    pub fn clear(&mut self) {
        self.start("gate", 0);
        self.anchors.clear();
    }

    /// A finished drag: from a system on the route it rewrites the route from there, from anywhere
    /// else it is a new route.
    pub fn take(&mut self, kind: &'static str, from: i64, to: i64) {
        match self.anchors.iter().position(|&a| a == from) {
            Some(at) if self.anchors.len() > 1 => {
                self.anchors.truncate(at + 1);
                self.anchors.push(to);
            }
            _ => {
                self.start(kind, from);
                self.anchors.push(to);
            }
        }
        self.kind = kind;
        self.replan();
    }

    pub fn set_dest(&mut self, sid: i64) {
        if self.anchors.len() > 1 {
            self.anchors.pop();
        }
        self.anchors.push(sid);
        self.replan();
    }

    pub fn add_waypoint(&mut self, sid: i64) {
        let at = self.anchors.len().saturating_sub(1);
        self.anchors.insert(at, sid);
        self.replan();
    }

    fn replan(&mut self) {
        self.leg_pick.truncate(self.anchors.len().saturating_sub(1));
        self.stale = true;
    }

    /// Works the route out again if anything changed.
    pub fn update(&mut self, input: &PlanInput) {
        if !self.stale {
            return;
        }
        self.stale = false;
        let avoid = self.avoid();
        let no_holes = HashMap::new();
        let holes = if self.prefs.via_holes && self.kind != "jump" { input.holes } else { &no_holes };
        let class = &SHIP_CLASSES[self.prefs.ship.min(SHIP_CLASSES.len() - 1)];
        let (legs, mut opts) = route::chain(
            input.geo,
            input.coords,
            &self.anchors,
            self.kind,
            class,
            self.prefs.jdc,
            self.prefs.jfc,
            TITAN_LY,
            self.titan_at_start,
            &self.titans,
            self.titan_self_jump,
            true,
            &avoid,
            holes,
            &self.leg_pick,
            &self.forks,
        );
        route::annotate(&mut opts, &HashMap::new());
        route::mark_anchors(&mut opts, &self.anchors);
        route::mark_forks(&mut opts, input.geo, true, &avoid, holes);
        self.legs = legs;
        self.opts = opts;
    }

    fn toggle_always(&mut self, sid: i64) {
        let list = if self.kind == "jump" { &mut self.prefs.avoid_jump } else { &mut self.prefs.avoid_gate };
        match list.iter().position(|&x| x == sid) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(sid),
        }
        self.dirty = true;
        self.replan();
    }

    /// The route part of a system's right-click menu.
    pub fn system_menu(&mut self, ui: &mut egui::Ui, sid: i64) {
        let at = self.anchors.iter().position(|&a| a == sid);
        if self.active() {
            match at {
                None => {
                    if ui.button("Set as Destination").clicked() {
                        self.set_dest(sid);
                        ui.close();
                    }
                    if self.anchors.len() > 1 && ui.button("Add Waypoint").clicked() {
                        self.add_waypoint(sid);
                        ui.close();
                    }
                }
                Some(i) if i > 0 => {
                    let last = i == self.anchors.len() - 1;
                    if ui.button(if last { "Remove Destination" } else { "Remove Waypoint" }).clicked() {
                        self.anchors.remove(i);
                        self.replan();
                        ui.close();
                    }
                }
                _ => {}
            }
            if ui.button(egui::RichText::new("Clear Route").color(spai_ui::theme::standing::HOSTILE)).clicked() {
                self.clear();
                ui.close();
            }
            if self.kind == "titan" {
                let t = self.titans.contains(&sid);
                if ui.button(if t { "Not a titan system" } else { "Set as titan system" }).clicked() {
                    self.titans.retain(|&x| x != sid);
                    if !t {
                        self.titans.push(sid);
                    }
                    self.replan();
                    ui.close();
                }
            }
            ui.separator();
            let once = self.avoid_once.contains(&sid);
            if ui.button(if once { "Stop avoiding here" } else { "Avoid for this route" }).clicked() {
                if once {
                    self.avoid_once.remove(&sid);
                } else {
                    self.avoid_once.insert(sid);
                }
                self.replan();
                ui.close();
            }
            let always = if self.kind == "jump" { &self.prefs.avoid_jump } else { &self.prefs.avoid_gate }.contains(&sid);
            if ui.button(if always { "Stop avoiding always" } else { "Avoid always" }).clicked() {
                self.toggle_always(sid);
                ui.close();
            }
            ui.separator();
        }
        let verb = if self.active() { "Restart as" } else { "Start" };
        for (kind, name) in [("gate", "Gate Route"), ("jump", "Jump Route"), ("titan", "Titan Route")] {
            if ui.button(format!("{verb} {name}")).clicked() {
                self.start(kind, sid);
                ui.close();
            }
        }
    }

    /// The ring of choices where a route drag was let go.
    pub fn link_menu(&mut self, ui: &mut egui::Ui) {
        let Some((from, to, at)) = self.link_menu else { return };
        // A drag off the destination adds a waypoint to a route that already has its kind.
        if self.anchors.len() > 1 && self.anchors.contains(&from) {
            self.link_menu = None;
            let kind = self.kind;
            self.take(kind, from, to);
            return;
        }
        const R: f32 = 58.0;
        const BTN: egui::Vec2 = egui::vec2(124.0, 28.0);
        let opts: [(&'static str, &str, &str); 4] =
            [("gate", icon::SIGN_IN, "Gate route"), ("jump", icon::SPIRAL, "Jump route"), ("titan", icon::CROSSHAIR_SIMPLE, "Titan route"), ("cancel", icon::X, "Cancel")];
        let half = egui::vec2(R + BTN.x / 2.0 + 8.0, R + BTN.y / 2.0 + 8.0);
        let mut chose: Option<&'static str> = None;
        let area = egui::Area::new(egui::Id::new("web_link_menu")).order(egui::Order::Foreground).fixed_pos(at - half).show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(half * 2.0, egui::Sense::hover());
            let c = rect.center();
            let v = ui.visuals().clone();
            let p = ui.painter();
            p.circle_filled(c, R + 16.0, v.window_fill.gamma_multiply(0.94));
            p.circle_stroke(c, R + 16.0, egui::Stroke::new(1.0, v.window_stroke.color));
            p.circle_filled(c, 3.5, v.hyperlink_color);
            for (idx, (kind, glyph, label)) in opts.iter().enumerate() {
                let a = (idx as f32 / opts.len() as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let br = egui::Rect::from_center_size(c + egui::vec2(a.cos() * R, a.sin() * R), BTN);
                let text = egui::RichText::new(format!("{glyph}  {label}"));
                let text = if *kind == "cancel" { text.color(v.weak_text_color()) } else { text };
                if ui.put(br, egui::Button::new(text)).clicked() {
                    chose = Some(kind);
                }
            }
            rect
        });
        if chose.is_none() && ui.input(|i| i.pointer.any_click()) {
            let inside = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| area.inner.contains(p));
            if !inside {
                self.link_menu = None;
            }
        }
        if let Some(kind) = chose {
            self.link_menu = None;
            if kind != "cancel" {
                self.take(kind, from, to);
            }
        }
    }

    /// The route panel: what is planned, its settings, alternatives and hops.
    pub fn panel(&mut self, ui: &mut egui::Ui, geo: &Systems) {
        let name = |id: i64| geo.info_of(id).map_or_else(|| id.to_string(), |i| i.name.clone());
        if !self.active() {
            ui.label(egui::RichText::new("Drag from one system to another to plan a route, or right-click a system to start one.").weak());
            return;
        }
        ui.horizontal(|ui| {
            ui.strong(match self.kind {
                "jump" => "Jump route",
                "titan" => "Titan route",
                _ => "Gate route",
            });
            if ui.small_button(icon::X).on_hover_text("Clear the route").clicked() {
                self.clear();
            }
        });
        if !self.active() {
            return;
        }
        let mut drop: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, &id) in self.anchors.iter().enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new(icon::ARROW_RIGHT).weak());
                }
                let t = egui::RichText::new(name(id)).strong();
                ui.label(if i == 0 || i == self.anchors.len() - 1 { t.color(ui.visuals().hyperlink_color) } else { t });
                if i > 0 && ui.small_button(icon::X).on_hover_text("Remove").clicked() {
                    drop = Some(i);
                }
            }
        });
        if let Some(i) = drop {
            self.anchors.remove(i);
            self.replan();
        }
        if self.anchors.len() < 2 {
            ui.label(egui::RichText::new("Drag from the start to where you are going, or right-click a system: Set as Destination.").weak());
            return;
        }
        if self.kind == "titan" {
            if ui.checkbox(&mut self.titan_at_start, "Titan is in the starting system").changed() {
                self.replan();
            }
            if self.titan_at_start
                && ui
                    .checkbox(&mut self.titan_self_jump, "Titan may reposition first")
                    .on_hover_text("The titan jumps somewhere that bridges better and the fleet gates out to meet it.")
                    .changed()
            {
                self.replan();
            }
            if self.titans.is_empty() && !self.titan_at_start {
                ui.label(egui::RichText::new("Right-click systems where titans are: Set as titan system.").weak());
            }
        }
        if self.kind != "jump" && ui.checkbox(&mut self.prefs.via_holes, "Route via the group's wormholes").changed() {
            self.dirty = true;
            self.replan();
        }
        if self.kind == "jump" {
            let class = &SHIP_CLASSES[self.prefs.ship.min(SHIP_CLASSES.len() - 1)];
            egui::CollapsingHeader::new(format!("{}  {} \u{b7} {:.1} ly", icon::SPIRAL, class.name, max_range_ly(class, self.prefs.jdc))).id_salt("web_route_ship").show(ui, |ui| {
                egui::ComboBox::from_id_salt("web_jump_ship").selected_text(class.name).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                    for (i, c) in SHIP_CLASSES.iter().enumerate() {
                        if ui.selectable_value(&mut self.prefs.ship, i, c.name).changed() {
                            self.dirty = true;
                            self.replan();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("JDC").on_hover_text("Jump Drive Calibration (range)");
                    if ui.add(egui::DragValue::new(&mut self.prefs.jdc).range(0..=5)).changed() {
                        self.dirty = true;
                        self.replan();
                    }
                    ui.label("JFC").on_hover_text("Jump Fuel Conservation (fuel)");
                    if ui.add(egui::DragValue::new(&mut self.prefs.jfc).range(0..=5)).changed() {
                        self.dirty = true;
                        self.replan();
                    }
                });
            });
        }
        let listed = route::avoided(geo, &self.avoid());
        if !listed.is_empty() {
            let mut stop: Option<(i64, bool)> = None;
            egui::CollapsingHeader::new(format!("{}  avoiding {} system{}", icon::EYE_SLASH, listed.len(), if listed.len() == 1 { "" } else { "s" }))
                .id_salt("web_route_avoid")
                .show(ui, |ui| {
                    for a in &listed {
                        ui.horizontal(|ui| {
                            ui.label(&a.name);
                            if a.always {
                                ui.label(egui::RichText::new("always").weak());
                            }
                            if ui.small_button(icon::X).on_hover_text("Stop avoiding").clicked() {
                                stop = Some((a.id, a.always));
                            }
                        });
                    }
                });
            if let Some((id, always)) = stop {
                if always {
                    self.toggle_always(id);
                } else {
                    self.avoid_once.remove(&id);
                    self.replan();
                }
            }
        }
        let Some(o) = self.opts.first().cloned() else {
            ui.separator();
            ui.label(egui::RichText::new("No route with these settings.").color(spai_ui::theme::standing::WARNING));
            return;
        };
        ui.separator();
        let mut head = format!("{} jumps", o.jumps);
        if o.gates > 0 {
            head.push_str(&format!(" \u{b7} {} gates", o.gates));
        }
        if o.total_ly > 0.0 {
            head.push_str(&format!(" \u{b7} {:.1} ly", o.total_ly));
        }
        ui.label(egui::RichText::new(head).strong());
        if let Some(saved) = o.saved {
            ui.label(egui::RichText::new(format!("saves {saved} gate{}", if saved == 1 { "" } else { "s" })).color(spai_ui::theme::standing::FRIENDLY));
        }
        if let Some(d) = &o.detour {
            ui.label(egui::RichText::new(format!("{}  {d}", icon::EYE_SLASH)).color(spai_ui::theme::standing::WARNING));
        }
        if let Some(tj) = &o.titan_jump {
            ui.label(egui::RichText::new(format!("{}  Titan jumps {} {} {}, {:.1} ly", icon::STAR_FOUR, tj.from_name, icon::ARROW_RIGHT, tj.to_name, tj.ly)).color(egui::Color32::from_rgb(0xFF, 0x7A, 0x3D)));
        }
        if let Some(n) = &o.note {
            ui.label(egui::RichText::new(n).weak());
        }
        // Alternatives, per leg that has several equally long ways.
        let mut pick: Option<(usize, usize)> = None;
        for (i, l) in self.legs.iter().enumerate() {
            if l.options.len() < 2 || l.whole_route {
                continue;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{} {} {}", l.from_name, icon::ARROW_RIGHT, l.to_name)).weak());
                for (k, opt) in l.options.iter().enumerate() {
                    let on = self.leg_pick.get(i).copied().unwrap_or(0) == k;
                    if ui.menu_label(on, &opt.label).clicked() {
                        pick = Some((i, k));
                    }
                }
            });
        }
        if let Some((i, k)) = pick {
            if self.leg_pick.len() <= i {
                self.leg_pick.resize(i + 1, 0);
            }
            self.leg_pick[i] = k;
            self.stale = true;
        }
        ui.separator();
        let mut act: Option<(i64, &'static str)> = None;
        let mut fork_now: Option<(i64, i64)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, h) in o.hops.iter().enumerate() {
                let frame = if h.anchor {
                    egui::Frame::new().fill(ui.visuals().hyperlink_color.gamma_multiply(0.16)).inner_margin(egui::Margin::symmetric(4, 1))
                } else {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(4, 1))
                };
                frame.show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        ui.menu_button(icon::DOTS_THREE, |ui| {
                            if !h.anchor {
                                let on = self.avoid_once.contains(&h.id);
                                if ui.button(if on { "Stop avoiding" } else { "Avoid this system" }).clicked() {
                                    act = Some((h.id, "avoid"));
                                    ui.close();
                                }
                                if ui.button("Add waypoint here").clicked() {
                                    act = Some((h.id, "waypoint"));
                                    ui.close();
                                }
                            }
                            if self.kind == "titan" {
                                let t = self.titans.contains(&h.id);
                                if ui.button(if t { "Not a titan system" } else { "Set as titan system" }).clicked() {
                                    act = Some((h.id, "titan"));
                                    ui.close();
                                }
                            }
                        });
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new(&h.name).color(spai_ui::colors::security_color(h.security)).strong());
                                let tail = if i == 0 {
                                    "start".to_owned()
                                } else {
                                    match h.kind {
                                        2 => format!("jump {:.1} ly", h.ly.unwrap_or_default()),
                                        1 => "ansiblex".to_owned(),
                                        _ => "gate".to_owned(),
                                    }
                                };
                                ui.label(egui::RichText::new(tail).weak());
                                if let (Some(fuel), Some(fat), Some(react)) = (h.fuel, h.fatigue_min, h.reactivation_min) {
                                    ui.label(egui::RichText::new(format!("{} iso \u{b7} fatigue {} \u{b7} ready in {}", fuel.round() as i64, minutes(fat), minutes(react))).weak());
                                }
                                if !h.fork.is_empty() {
                                    let taken = o.hops.get(i + 1).map(|n| n.id);
                                    ui.label(egui::RichText::new(icon::ARROWS_SPLIT).color(ui.visuals().hyperlink_color)).on_hover_text("Ways on from here, all the same length");
                                    for alt in &h.fork {
                                        let on = taken == Some(alt.id);
                                        if ui.menu_label(on, &alt.name).on_hover_text(if on { "The way this route goes" } else { "Go this way instead" }).clicked() && !on {
                                            fork_now = Some((h.id, alt.id));
                                        }
                                    }
                                }
                            });
                        });
                    });
                });
            }
        });
        if let Some((at, next)) = fork_now {
            self.forks.insert(at, next);
            self.stale = true;
        }
        match act {
            Some((id, "avoid")) => {
                if !self.avoid_once.remove(&id) {
                    self.avoid_once.insert(id);
                }
                self.replan();
            }
            Some((id, "waypoint")) => self.add_waypoint(id),
            Some((id, "titan")) => {
                let t = self.titans.contains(&id);
                self.titans.retain(|&x| x != id);
                if !t {
                    self.titans.push(id);
                }
                self.replan();
            }
            _ => {}
        }
    }

    /// Systems the planner marks on the map: anchors and titans.
    pub fn marks(&self) -> (&[i64], &[i64]) {
        (&self.anchors, &self.titans)
    }
}

fn minutes(m: f64) -> String {
    let m = m.round() as i64;
    if m >= 60 {
        format!("{}h {:02}m", m / 60, m % 60)
    } else {
        format!("{m}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_plans_and_a_drag_off_the_destination_extends_it() {
        let geo = spai_core::test_support::small_universe(&[]);
        let coords: Vec<MapSystem> = Vec::new();
        let holes = HashMap::new();
        let input = PlanInput { geo: &geo, coords: &coords, holes: &holes };
        let mut p = RoutePlan::default();
        p.take("gate", 30_004_759, 30_004_608);
        p.update(&input);
        assert_eq!(p.route().map(|o| o.path.clone()), Some(vec![30_004_759, 30_004_608]));
        p.take("gate", 30_004_608, 30_003_704);
        p.update(&input);
        assert_eq!(p.anchors, vec![30_004_759, 30_004_608, 30_003_704], "the destination became a waypoint");
        assert_eq!(p.route().unwrap().path.len(), 3);
        p.take("gate", 30_003_704, 30_004_759);
        assert_eq!(p.anchors.len(), 4);
        p.take("jump", 30_000_142, 30_004_759);
        assert_eq!((p.kind, p.anchors.clone()), ("jump", vec![30_000_142, 30_004_759]), "from elsewhere: a new route");
    }

    #[test]
    fn avoiding_always_is_kept_per_kind_and_marks_the_prefs_for_saving() {
        let mut p = RoutePlan::default();
        p.start("gate", 1);
        p.toggle_always(5);
        assert!(p.dirty && p.prefs.avoid_gate == vec![5] && p.prefs.avoid_jump.is_empty());
        p.start("jump", 1);
        assert_eq!(p.prefs.avoid_gate, vec![5], "restarting keeps the lists");
    }
}
