//! Routes planned on the web map, the desktop's way: drag from a system to another and pick a gate,
//! jump or mixed route, a mixed one asking gate or jump for every leg; right-click a system to start
//! a route there, set a destination, add a waypoint or avoid it; pick alternatives per leg and
//! branches at forks. The planning is the shared `spai_core::route`, so both maps answer alike.

use std::collections::{HashMap, HashSet};

use egui_phosphor::regular as icon;
use spai_core::geo::Systems;
use spai_core::jumproute::{max_range_ly, SHIP_CLASSES};
use spai_core::map::MapSystem;
use spai_core::route::{self, Avoid, LegChoice, Picks, RouteOption};
use spai_core::wormholes::{HoleKind, Mass, RouteLimits, ShipSize, TimeLeft, Wormhole};
use spai_ui::widgets::SteadySelect as _;

/// What the planner keeps between visits: the avoid lists, the jump ship and which holes routes may
/// use.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PlanPrefs {
    pub avoid_gate: Vec<i64>,
    pub avoid_jump: Vec<i64>,
    pub ship: usize,
    pub jdc: u32,
    pub jfc: u32,
    pub via_holes: bool,
    /// The kinds of hole routes may go through, by `HoleKind::code`.
    pub wh_kinds: Vec<String>,
    /// The least mass, time and size a hole on a route must have: codes, empty for any.
    pub wh_min_mass: String,
    pub wh_min_time: String,
    pub wh_min_size: String,
    /// Holes, by uid, and systems whose holes routes never use.
    pub wh_off_holes: Vec<String>,
    pub wh_off_systems: Vec<i64>,
}

impl Default for PlanPrefs {
    fn default() -> Self {
        PlanPrefs {
            avoid_gate: Vec::new(),
            avoid_jump: Vec::new(),
            ship: 0,
            jdc: 5,
            jfc: 5,
            via_holes: true,
            wh_kinds: HoleKind::ALL.iter().map(|k| k.code().to_owned()).collect(),
            wh_min_mass: String::new(),
            wh_min_time: String::new(),
            wh_min_size: String::new(),
            wh_off_holes: Vec::new(),
            wh_off_systems: Vec::new(),
        }
    }
}

impl PlanPrefs {
    pub fn toggle_off_hole(&mut self, uid: &str) {
        match self.wh_off_holes.iter().position(|u| u == uid) {
            Some(i) => {
                self.wh_off_holes.remove(i);
            }
            None => self.wh_off_holes.push(uid.to_owned()),
        }
    }

    pub fn toggle_off_system(&mut self, id: i64) {
        match self.wh_off_systems.iter().position(|s| *s == id) {
            Some(i) => {
                self.wh_off_systems.remove(i);
            }
            None => self.wh_off_systems.push(id),
        }
    }

    /// A button naming the kinds of hole routes may use, opening the list to pick them. Returns
    /// whether any changed.
    pub fn kinds_button(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let on = |k: HoleKind, p: &PlanPrefs| p.wh_kinds.iter().any(|c| c == k.code());
        let n = HoleKind::ALL.iter().filter(|k| on(**k, self)).count();
        let text = match n {
            0 => "No holes".to_owned(),
            n if n == HoleKind::ALL.len() => "All kinds".to_owned(),
            1 => HoleKind::ALL.iter().find(|k| on(**k, self)).map_or_else(String::new, |k| k.label().to_owned()),
            n => format!("{n} of {} kinds", HoleKind::ALL.len()),
        };
        let button = ui.button(format!("{text}  {}", icon::CARET_DOWN));
        // Stays open while kinds are picked: each click is one of several choices.
        egui::Popup::from_toggle_button_response(&button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for k in HoleKind::ALL {
                let was = on(k, self);
                if ui.menu_label(was, k.label()).clicked() {
                    if was {
                        self.wh_kinds.retain(|c| c != k.code());
                    } else {
                        self.wh_kinds.push(k.code().to_owned());
                    }
                    changed = true;
                }
            }
        });
        changed
    }

    /// A system's holes, each to switch on or off for routes, and the whole system at once, as the
    /// desktop offers them. Nothing for a system without a hole.
    pub fn hole_switches(&mut self, ui: &mut egui::Ui, sid: i64, geo: &Systems, holes: &[Wormhole]) -> bool {
        const LISTED: usize = 8;
        let here: Vec<(String, String)> = holes
            .iter()
            .filter(|w| w.system_id == sid || w.dest_system_id == Some(sid))
            .map(|w| {
                let (sig, far) = if w.system_id == sid { (&w.signature, w.dest_system_id) } else { (&w.dest_signature, Some(w.system_id)) };
                let far = far.and_then(|f| geo.info_of(f)).map_or_else(|| w.dest.label().to_owned(), |i| i.name.clone());
                let sig = sig.as_deref().map(|s| format!("{} ", s.chars().take(3).collect::<String>())).unwrap_or_default();
                (w.uid.clone(), format!("{sig}to {far}"))
            })
            .collect();
        let off = self.wh_off_systems.contains(&sid);
        if here.is_empty() && !off {
            return false;
        }
        let mut changed = false;
        let name = geo.info_of(sid).map_or_else(|| format!("#{sid}"), |i| i.name.clone());
        ui.separator();
        ui.label(egui::RichText::new("Wormholes routes may use").weak());
        let mut all = !off;
        if ui.checkbox(&mut all, format!("Any hole in {name}")).on_hover_text("Off keeps every hole here off routes, those found later too").changed() {
            self.toggle_off_system(sid);
            changed = true;
        }
        let mut flip: Option<String> = None;
        ui.add_enabled_ui(all, |ui| {
            for (uid, label) in here.iter().take(LISTED) {
                let mut on = !self.wh_off_holes.contains(uid);
                if ui.checkbox(&mut on, label).on_hover_text("Both sides of this hole").changed() {
                    flip = Some(uid.clone());
                }
            }
        });
        if let Some(uid) = flip {
            self.toggle_off_hole(&uid);
            changed = true;
        }
        if here.len() > LISTED {
            ui.label(egui::RichText::new(format!("and {} more", here.len() - LISTED)).weak());
        }
        changed
    }

    fn limits(&self) -> RouteLimits {
        RouteLimits {
            min_mass: Mass::from_code(&self.wh_min_mass),
            time_below: TimeLeft::ALL.into_iter().find(|t| t.code() == self.wh_min_time),
            min_size: ShipSize::from_code(&self.wh_min_size),
        }
    }

    fn hole_off(&self, w: &Wormhole) -> bool {
        self.wh_off_holes.contains(&w.uid) || self.wh_off_systems.contains(&w.system_id) || w.dest_system_id.is_some_and(|b| self.wh_off_systems.contains(&b))
    }

    /// The holes routes may take, both ways: far side known, a kind allowed, within the limits and
    /// not switched off.
    pub fn hole_graph(&self, geo: &Systems, holes: &[Wormhole], now: i64) -> HashMap<i64, Vec<i64>> {
        let limits = self.limits();
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        if !self.via_holes {
            return adj;
        }
        for w in holes {
            let Some(b) = w.dest_system_id else { continue };
            let kind = HoleKind::of(geo, w.system_id, b, w.is_drifter);
            if self.wh_kinds.iter().any(|k| k == kind.code()) && limits.allows(w, now) && !self.hole_off(w) {
                adj.entry(w.system_id).or_default().push(b);
                adj.entry(b).or_default().push(w.system_id);
            }
        }
        adj
    }
}

#[derive(Default)]
pub struct RoutePlan {
    /// "gate", "jump" or "mixed": how new legs are flown, a mixed route asking each time.
    pub kind: &'static str,
    /// Start, waypoints, destination.
    pub anchors: Vec<i64>,
    /// How each leg between two anchors is flown, "gate" or "jump".
    pub leg_kinds: Vec<&'static str>,
    pub opts: Vec<RouteOption>,
    pub legs: Vec<LegChoice>,
    leg_pick: Vec<usize>,
    forks: Picks,
    avoid_once: HashSet<i64>,
    pub prefs: PlanPrefs,
    /// The prefs changed and want saving.
    pub dirty: bool,
    /// A drag started on this system.
    pub link: Option<i64>,
    /// A drag let go on a system: where from, where to, where on screen, and whether the menu asks
    /// only how to fly this leg of a mixed route.
    pub link_menu: Option<(i64, i64, egui::Pos2, bool)>,
    /// The anchors, or what they are planned with, changed since the route was worked out.
    stale: bool,
    /// Characters added with scopes, as the app last listed them.
    pub pilots: Vec<Pilot>,
    /// Something to do with ESI that the app carries out.
    pub request: Option<PlanRequest>,
    /// How the last request went.
    pub note: Option<Result<String, String>>,
    pilot: Option<i64>,
    /// This route's own Ansiblex limit over the group's, 0 for none at all, as the desktop's.
    pub zone: Option<u8>,
    /// The graph laid for `zone`, and the graph and zone it was laid from.
    zone_graph: Option<(usize, u8, Systems)>,
    /// The group's limit and whether it has bridges, for the panel.
    group_zone: u8,
    has_bridges: bool,
}

/// A character the route can be set for or whose skills can be read.
#[derive(Clone, Debug, PartialEq)]
pub struct Pilot {
    pub char_id: i64,
    pub name: String,
    /// Where they are, when the location scope says.
    pub system: Option<i64>,
    pub waypoints: bool,
    pub skills: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanRequest {
    /// Set these systems as the character's autopilot route.
    SetRoute(i64, Vec<i64>),
    /// Read the character's jump skills into the planner.
    Skills(i64),
}

/// What a route needs from the map to be worked out.
pub struct PlanInput<'a> {
    pub geo: &'a Systems,
    /// Every system with its true position, for jump ranges.
    pub coords: &'a [MapSystem],
    /// The group's live holes.
    pub holes: &'a [Wormhole],
    /// The group's Ansiblexes, to re-lay for a route's own zone limit.
    pub network: &'a crate::starmap::Network,
}

/// The way a mode flies a leg nobody asked about: a mixed route's default is gates.
fn default_leg(mode: &str) -> &'static str {
    if mode == "jump" { "jump" } else { "gate" }
}

fn mode_of(kind: &str) -> &'static str {
    match kind {
        "jump" => "jump",
        "mixed" => "mixed",
        _ => "gate",
    }
}

impl RoutePlan {
    pub fn active(&self) -> bool {
        !self.anchors.is_empty()
    }

    pub fn route(&self) -> Option<&RouteOption> {
        self.opts.first()
    }

    /// How leg `i` is flown.
    pub fn leg_kind(&self, i: usize) -> &'static str {
        self.leg_kinds.get(i).copied().unwrap_or(default_leg(self.kind))
    }

    /// Whether any leg is flown as `kind`; with no leg yet, whether the next one would be.
    pub fn has(&self, kind: &str) -> bool {
        let n = self.anchors.len().saturating_sub(1);
        if n == 0 {
            return default_leg(self.kind) == kind;
        }
        (0..n).any(|i| self.leg_kind(i) == kind)
    }

    /// The persistent avoid lists this route is planned against: gate legs the gate list, jump legs
    /// the jump list.
    fn lists(&self) -> Vec<bool> {
        [(false, "gate"), (true, "jump")].into_iter().filter(|(_, k)| self.has(k)).map(|(j, _)| j).collect()
    }

    fn avoid(&self, jump: bool) -> Avoid {
        let always = if jump { &self.prefs.avoid_jump } else { &self.prefs.avoid_gate };
        Avoid { always: always.iter().copied().collect(), once: self.avoid_once.clone() }
    }

    pub fn start(&mut self, kind: &str, sid: i64) {
        *self = RoutePlan {
            kind: mode_of(kind),
            anchors: vec![sid],
            prefs: std::mem::take(&mut self.prefs),
            dirty: self.dirty,
            pilots: std::mem::take(&mut self.pilots),
            pilot: self.pilot,
            ..Default::default()
        };
    }

    pub fn clear(&mut self) {
        self.start("gate", 0);
        self.anchors.clear();
    }

    /// A finished drag: from a system on the route it rewrites the route from there, from anywhere
    /// else it is a new route. `leg` is how the dragged part is flown.
    pub fn take(&mut self, mode: &str, leg: &'static str, from: i64, to: i64) {
        match self.anchors.iter().position(|&a| a == from) {
            Some(at) if self.anchors.len() > 1 => {
                self.anchors.truncate(at + 1);
                self.anchors.push(to);
                self.leg_kinds.truncate(at);
                self.leg_kinds.push(leg);
            }
            _ => {
                self.start(mode, from);
                self.anchors.push(to);
                self.leg_kinds = vec![leg];
            }
        }
        self.kind = mode_of(mode);
        self.replan();
    }

    /// Makes the route mixed when `leg` differs from a plain route's kind.
    fn mix_in(&mut self, leg: &str) {
        if self.kind != "mixed" && leg != self.kind {
            self.kind = "mixed";
        }
    }

    pub fn set_dest(&mut self, sid: i64, leg: Option<&'static str>) {
        if self.anchors.is_empty() {
            if let Some(from) = self.here().filter(|h| *h != sid) {
                self.anchors.push(from);
            }
        }
        if self.anchors.len() > 1 {
            self.anchors.pop();
            if let (Some(k), Some(last)) = (leg, self.leg_kinds.last_mut()) {
                *last = k;
            }
        } else if !self.anchors.is_empty() {
            self.leg_kinds = vec![leg.unwrap_or(default_leg(self.kind))];
        }
        self.anchors.push(sid);
        if let Some(k) = leg {
            self.mix_in(k);
        }
        self.replan();
    }

    /// A waypoint goes in before the destination; `leg` is how to fly to it, and the leg on keeps
    /// its way.
    pub fn add_waypoint(&mut self, sid: i64, leg: Option<&'static str>) {
        let at = self.anchors.len().saturating_sub(1);
        self.anchors.insert(at, sid);
        let k = leg.unwrap_or(default_leg(self.kind));
        let into = at.saturating_sub(1).min(self.leg_kinds.len());
        self.leg_kinds.insert(into, k);
        self.mix_in(k);
        self.replan();
    }

    /// Takes anchor `i` out with the leg into it.
    pub fn remove_anchor(&mut self, i: usize) {
        if i == 0 || i >= self.anchors.len() {
            return;
        }
        self.anchors.remove(i);
        if i - 1 < self.leg_kinds.len() {
            self.leg_kinds.remove(i - 1);
        }
        self.replan();
    }

    /// Flies leg `i` the other way; a plain route with a leg changed is a mixed one.
    pub fn set_leg_kind(&mut self, i: usize, kind: &'static str) {
        let n = self.anchors.len().saturating_sub(1);
        self.leg_kinds = (0..n).map(|k| self.leg_kind(k)).collect();
        if let Some(k) = self.leg_kinds.get_mut(i) {
            *k = kind;
        }
        self.mix_in(kind);
        self.replan();
    }

    /// Gate or jump for the whole route, or mixed, which keeps each leg as it is.
    pub fn set_mode(&mut self, kind: &str) {
        self.kind = mode_of(kind);
        if self.kind != "mixed" {
            let k = self.kind;
            self.leg_kinds.iter_mut().for_each(|l| *l = k);
        }
        self.replan();
    }

    /// Where the chosen character is, else the first added one ESI has placed.
    pub fn here(&self) -> Option<i64> {
        let chosen = self.pilot.and_then(|id| self.pilots.iter().find(|p| p.char_id == id)).and_then(|p| p.system);
        chosen.or_else(|| self.pilots.iter().find_map(|p| p.system))
    }

    fn replan(&mut self) {
        self.note = None;
        self.leg_pick.truncate(self.anchors.len().saturating_sub(1));
        self.stale = true;
    }

    /// The holes changed: a route through one may no longer hold.
    pub fn holes_changed(&mut self) {
        if self.anchors.len() > 1 {
            self.stale = true;
        }
    }

    /// Works the route out again if anything changed.
    pub fn update(&mut self, input: &PlanInput) {
        self.group_zone = input.network.max_zone.max(1);
        self.has_bridges = !input.network.bridges.is_empty();
        if !self.stale {
            return;
        }
        self.stale = false;
        let base_key = input.geo as *const Systems as usize;
        let geo: &Systems = match self.zone.filter(|z| *z != self.group_zone && self.has_bridges) {
            None => input.geo,
            Some(z) => {
                if self.zone_graph.as_ref().is_none_or(|(k, zz, _)| *k != base_key || *zz != z) {
                    let g = if z == 0 {
                        input.geo.gates_only()
                    } else {
                        spai_core::ansiblex::with_max_zone(input.geo, &input.network.list(), &input.network.capital, z)
                    };
                    self.zone_graph = Some((base_key, z, g));
                }
                &self.zone_graph.as_ref().unwrap().2
            }
        };
        let input = &PlanInput { geo, ..*input };
        let (avoid_gate, avoid_jump) = (self.avoid(false), self.avoid(true));
        let holes = self.prefs.hole_graph(input.geo, input.holes, spai_core::clock::utc().timestamp());
        let class = &SHIP_CLASSES[self.prefs.ship.min(SHIP_CLASSES.len() - 1)];
        let kinds: Vec<&str> = (0..self.anchors.len().saturating_sub(1)).map(|i| self.leg_kind(i)).collect();
        let (legs, mut opts) = route::chain(
            input.geo,
            input.coords,
            &self.anchors,
            &kinds,
            class,
            self.prefs.jdc,
            self.prefs.jfc,
            true,
            &avoid_gate,
            &avoid_jump,
            &holes,
            &self.leg_pick,
            &self.forks,
        );
        route::annotate(&mut opts, &HashMap::new());
        route::mark_anchors(&mut opts, &self.anchors);
        route::mark_forks(&mut opts, input.geo, true, &avoid_gate, &holes);
        self.legs = legs;
        self.opts = opts;
    }

    fn always_avoided(&self, sid: i64) -> bool {
        self.lists().into_iter().any(|jump| if jump { &self.prefs.avoid_jump } else { &self.prefs.avoid_gate }.contains(&sid))
    }

    /// Avoids `sid` always, or stops, in each list this route is planned against.
    fn toggle_always(&mut self, sid: i64) {
        let on = !self.always_avoided(sid);
        for jump in self.lists() {
            let list = if jump { &mut self.prefs.avoid_jump } else { &mut self.prefs.avoid_gate };
            list.retain(|&x| x != sid);
            if on {
                list.push(sid);
            }
        }
        self.dirty = true;
        self.replan();
    }


    /// The route part of a system's right-click menu.
    pub fn system_menu(&mut self, ui: &mut egui::Ui, sid: i64, geo: &Systems, holes: &[Wormhole]) {
        let at = self.anchors.iter().position(|&a| a == sid);
        if !self.active() && self.here().is_some_and(|h| h != sid) {
            for (leg, label) in [("gate", "Set as Destination (Gate)"), ("jump", "Set as Destination (Jump)")] {
                if ui.button(label).on_hover_text("From where your character is").clicked() {
                    self.kind = leg;
                    self.set_dest(sid, Some(leg));
                    ui.close();
                }
            }
        }
        if self.active() {
            // A mixed route asks how to fly there; a plain one flies it its own way.
            let ways: &[(Option<&'static str>, &str)] = if self.kind == "mixed" { &[(Some("jump"), " (Jump)"), (Some("gate"), " (Gate)")] } else { &[(None, "")] };
            match at {
                None => {
                    for (leg, tag) in ways {
                        if ui.button(format!("Set as Destination{tag}")).clicked() {
                            self.set_dest(sid, *leg);
                            ui.close();
                        }
                    }
                    if self.anchors.len() > 1 {
                        for (leg, tag) in ways {
                            if ui.button(format!("Add Waypoint{tag}")).clicked() {
                                self.add_waypoint(sid, *leg);
                                ui.close();
                            }
                        }
                    }
                }
                Some(i) if i > 0 => {
                    let last = i == self.anchors.len() - 1;
                    if ui.button(if last { "Remove Destination" } else { "Remove Waypoint" }).clicked() {
                        self.remove_anchor(i);
                        ui.close();
                    }
                    let (other, label) = if self.leg_kind(i - 1) == "jump" { ("gate", "Reach by Gate") } else { ("jump", "Reach by Jump") };
                    if ui.button(label).on_hover_text("Fly the leg into here the other way; the route becomes a mixed one").clicked() {
                        self.set_leg_kind(i - 1, other);
                        ui.close();
                    }
                }
                _ => {}
            }
            if ui.button(egui::RichText::new("Clear Route").color(spai_ui::theme::standing::HOSTILE)).clicked() {
                self.clear();
                ui.close();
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
            if ui.button(if self.always_avoided(sid) { "Stop avoiding always" } else { "Avoid always" }).clicked() {
                self.toggle_always(sid);
                ui.close();
            }
            ui.separator();
        }
        let verb = if self.active() { "Restart as" } else { "Start" };
        for (kind, name) in [("gate", "Gate Route"), ("jump", "Jump Route"), ("mixed", "Mixed Route")] {
            if ui.button(format!("{verb} {name}")).clicked() {
                self.start(kind, sid);
                ui.close();
            }
        }
        if self.prefs.hole_switches(ui, sid, geo, holes) {
            self.dirty = true;
            self.replan();
        }
    }

    /// The ring of choices where a route drag was let go: a new route's kind, or for a leg of a
    /// mixed route, gate or jump.
    pub fn link_menu(&mut self, ui: &mut egui::Ui) {
        let Some((from, to, at, leg_only)) = self.link_menu else { return };
        // A drag off a system of the route extends it: a plain route its own way, a mixed one asks.
        let extends = self.anchors.len() > 1 && self.anchors.contains(&from);
        if extends && !leg_only {
            if self.kind == "mixed" {
                self.link_menu = Some((from, to, at, true));
            } else {
                self.link_menu = None;
                let kind = self.kind;
                self.take(kind, default_leg(kind), from, to);
            }
            return;
        }
        const R: f32 = 58.0;
        const BTN: egui::Vec2 = egui::vec2(124.0, 28.0);
        let opts: Vec<(&'static str, &str, &str)> = if leg_only {
            vec![("gate", icon::SIGN_IN, "Gate"), ("jump", icon::SPIRAL, "Jump"), ("cancel", icon::X, "Cancel")]
        } else {
            vec![("gate", icon::SIGN_IN, "Gate route"), ("jump", icon::SPIRAL, "Jump route"), ("mixed", icon::SHUFFLE, "Mixed route"), ("cancel", icon::X, "Cancel")]
        };
        let half = egui::vec2(R + BTN.x / 2.0 + 8.0, R + BTN.y / 2.0 + 8.0);
        let mut chose: Option<&'static str> = None;
        let area = egui::Area::new(egui::Id::new("web_link_menu").with(leg_only)).order(egui::Order::Foreground).fixed_pos(at - half).show(ui.ctx(), |ui| {
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
        match chose {
            None => {}
            Some("cancel") => self.link_menu = None,
            // A new mixed route: its first leg is asked for like every later one.
            Some("mixed") => self.link_menu = Some((from, to, at, true)),
            Some(leg) if leg_only => {
                self.link_menu = None;
                let mode = if extends { self.kind } else { "mixed" };
                self.take(mode, default_leg(leg), from, to);
            }
            Some(kind) => {
                self.link_menu = None;
                self.take(kind, default_leg(kind), from, to);
            }
        }
    }

    /// The route's own Ansiblex limit over the group's, as the desktop offers it: none at all, or
    /// up to a zone.
    fn zone_combo(&mut self, ui: &mut egui::Ui) {
        if !self.has_bridges {
            return;
        }
        let group = self.group_zone;
        let current = self.zone.unwrap_or(group);
        let text = |z: u8| if z == 0 { "None".to_owned() } else { format!("Up to {}", spai_core::ansiblex::zone_label(z)) };
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Ansiblexes");
            egui::ComboBox::from_id_salt("web_route_zone")
                .selected_text(text(current))
                .show_ui(ui, |ui| {
                    for z in 0..=spai_core::ansiblex::MAX_ZONE {
                        let mut label = text(z);
                        if z == group {
                            label.push_str(" (group's)");
                        }
                        if ui.menu_label(current == z, label).clicked() && current != z {
                            self.zone = (z != group).then_some(z);
                            changed = true;
                        }
                    }
                })
                .response
                .on_hover_text("For this route only. The group's limit stays as it is.");
        });
        if changed {
            self.replan();
        }
    }

    /// The route panel: what is planned, its settings, alternatives and hops.
    pub fn panel(&mut self, ui: &mut egui::Ui, geo: &Systems) {
        let name = |id: i64| geo.info_of(id).map_or_else(|| id.to_string(), |i| i.name.clone());
        if !self.active() {
            ui.label(egui::RichText::new("Drag from one system to another to plan a route, or right-click a system to start one.").weak());
            self.zone_combo(ui);
            self.hole_options(ui);
            return;
        }
        ui.horizontal(|ui| {
            let mut mode: Option<&str> = None;
            for (kind, label) in [("gate", "Gates"), ("jump", "Jumps"), ("mixed", "Mixed")] {
                if ui.menu_label(self.kind == kind, label).clicked() && self.kind != kind {
                    mode = Some(kind);
                }
            }
            if let Some(k) = mode {
                self.set_mode(k);
            }
            if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text("Clear the route").clicked() {
                self.clear();
            }
        });
        if !self.active() {
            return;
        }
        let mut drop: Option<usize> = None;
        let mut flip: Option<(usize, &'static str)> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, &id) in self.anchors.iter().enumerate() {
                if i > 0 {
                    // How the leg into this anchor is flown, switched in place.
                    let (glyph, tip, other) = if self.leg_kind(i - 1) == "jump" { (icon::SPIRAL, "Jumped; click to gate it", "gate") } else { (icon::SIGN_IN, "Gated; click to jump it", "jump") };
                    if spai_ui::widgets::icon_button(ui, glyph).on_hover_text(tip).clicked() {
                        flip = Some((i - 1, other));
                    }
                }
                let t = egui::RichText::new(name(id)).strong();
                ui.label(if i == 0 || i == self.anchors.len() - 1 { t.color(ui.visuals().hyperlink_color) } else { t });
                if i > 0 && spai_ui::widgets::icon_button(ui, icon::X).on_hover_text("Remove").clicked() {
                    drop = Some(i);
                }
            }
        });
        if let Some(i) = drop {
            self.remove_anchor(i);
        }
        if let Some((i, k)) = flip {
            self.set_leg_kind(i, k);
        }
        if self.anchors.len() < 2 {
            ui.label(egui::RichText::new("Drag from the start to where you are going, or right-click a system: Set as Destination.").weak());
            return;
        }
        if self.has("gate") {
            self.zone_combo(ui);
            self.hole_options(ui);
        }
        if self.has("jump") {
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
                let skilled: Vec<(i64, String)> = self.pilots.iter().filter(|p| p.skills).map(|p| (p.char_id, p.name.clone())).collect();
                ui.horizontal_wrapped(|ui| {
                    for (id, name) in skilled {
                        if ui.button(format!("{name}'s skills")).on_hover_text("Read JDC and JFC from ESI").clicked() {
                            self.request = Some(PlanRequest::Skills(id));
                        }
                    }
                });
            });
        }
        let mut avoid = Avoid { always: HashSet::new(), once: self.avoid_once.clone() };
        for jump in self.lists() {
            avoid.always.extend(self.avoid(jump).always);
        }
        let listed = route::avoided(geo, &avoid);
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
                            if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text("Stop avoiding").clicked() {
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
        if let Some(d) = &o.detour {
            ui.label(egui::RichText::new(format!("{}  {d}", icon::EYE_SLASH)).color(spai_ui::theme::standing::WARNING));
        }
        if let Some(n) = &o.note {
            ui.label(egui::RichText::new(n).weak());
        }
        self.set_in_game(ui, &o);
        // A leg's equally long alternatives are picked at its forks, in the steps below.
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
                        if !h.anchor {
                            ui.menu_button(icon::DOTS_THREE, |ui| {
                                let on = self.avoid_once.contains(&h.id);
                                if ui.button(if on { "Stop avoiding" } else { "Avoid this system" }).clicked() {
                                    act = Some((h.id, "avoid"));
                                    ui.close();
                                }
                                if ui.button("Add waypoint here").clicked() {
                                    act = Some((h.id, if h.kind == 2 { "jump" } else { "gate" }));
                                    ui.close();
                                }
                            });
                        }
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new(&h.name).color(spai_ui::colors::security_color(h.security)).strong());
                                let (tail, colour) = spai_ui::star_map::hop_tail(h, i == 0);
                                let tail = egui::RichText::new(tail);
                                ui.label(match colour {
                                    Some(c) => tail.color(c),
                                    None => tail.weak(),
                                });
                                if let (Some(fuel), Some(fat), Some(react)) = (h.fuel, h.fatigue_min, h.reactivation_min) {
                                    ui.label(egui::RichText::new(format!("{} iso \u{b7} fatigue {} \u{b7} ready in {}", fuel.round() as i64, minutes(fat), minutes(react))).weak());
                                }
                                // A fork: the way this route goes, with the equally long others to pick from.
                                if !h.fork.is_empty() {
                                    let taken = o.hops.get(i + 1).map(|n| n.id);
                                    let current = h.fork.iter().find(|a| Some(a.id) == taken).map_or("?", |a| a.name.as_str());
                                    egui::ComboBox::from_id_salt(("web_route_fork", h.id))
                                        .selected_text(format!("{}  {current}", icon::ARROWS_SPLIT))
                                        .show_ui(ui, |ui| {
                                            for alt in &h.fork {
                                                let on = taken == Some(alt.id);
                                                if ui.menu_label(on, &alt.name).clicked() && !on {
                                                    fork_now = Some((h.id, alt.id));
                                                }
                                            }
                                        })
                                        .response
                                        .on_hover_text("Ways on from here, all the same length");
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
            Some((id, leg)) => self.add_waypoint(id, Some(leg)),
            None => {}
        }
    }

    /// Whether routes go through the group's holes, and which: kinds, mass, time left, size, and
    /// the holes switched off by hand.
    fn hole_options(&mut self, ui: &mut egui::Ui) {
        if ui.checkbox(&mut self.prefs.via_holes, "Route via the group's wormholes").changed() {
            self.dirty = true;
            self.replan();
        }
        if !self.prefs.via_holes {
            return;
        }
        let mut changed = false;
        egui::CollapsingHeader::new(format!("{}  Which holes", icon::FUNNEL)).id_salt("web_route_holes").show(ui, |ui| {
            let combo = |ui: &mut egui::Ui, id: &str, value: &mut String, items: &[(&str, &str, &str)]| -> bool {
                let mut changed = false;
                let current = items.iter().find(|i| i.0 == value.as_str()).unwrap_or(&items[0]).1;
                egui::ComboBox::from_id_salt(("web_wh_route", id)).selected_text(current).show_ui(ui, |ui| {
                    for (code, label, hint) in items {
                        if ui.menu_label(value == code, *label).on_hover_text(*hint).clicked() && value != code {
                            *value = (*code).to_owned();
                            changed = true;
                        }
                    }
                });
                changed
            };
            egui::Grid::new("web_wh_route_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Through");
                changed |= self.prefs.kinds_button(ui);
                ui.end_row();
                ui.label("Mass");
                changed |= combo(
                    ui,
                    "mass",
                    &mut self.prefs.wh_min_mass,
                    &[
                        ("", "Any", "Any mass left"),
                        (Mass::Reduced.code(), "Not critical", "Skip holes with under 10% mass left. Holes with their mass not read pass."),
                        (Mass::Fresh.code(), "Over 50%", "Only holes with over half their mass left. Holes with their mass not read pass."),
                    ],
                );
                ui.end_row();
                ui.label("Time left");
                changed |= combo(
                    ui,
                    "time",
                    &mut self.prefs.wh_min_time,
                    &[
                        ("", "Any", "Any time left, even holes that could close any moment"),
                        (TimeLeft::Expiring.code(), "Not expired", "Skip holes past their time"),
                        (TimeLeft::Under1h.code(), "1h or more", "Skip holes with under an hour left"),
                        (TimeLeft::Under4h.code(), "4h or more", "Skip holes with under 4 hours left"),
                        (TimeLeft::Under12h.code(), "12h or more", "Skip holes with under 12 hours left"),
                    ],
                );
                ui.end_row();
                ui.label("Size");
                changed |= combo(
                    ui,
                    "size",
                    &mut self.prefs.wh_min_size,
                    &[
                        ("", "Any", "Any hole size"),
                        (ShipSize::Medium.code(), "Medium or bigger", "Holes a cruiser fits through. Holes of unknown size and type pass."),
                        (ShipSize::Large.code(), "Large or bigger", "Holes a battleship fits through. Holes of unknown size and type pass."),
                        (ShipSize::XLarge.code(), "XL", "Holes a capital fits through. Holes of unknown size and type pass."),
                    ],
                );
                ui.end_row();
            });
            let off = self.prefs.wh_off_holes.len() + self.prefs.wh_off_systems.len();
            if off > 0 {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{off} switched off")).weak()).on_hover_text("Right-click a system with holes to switch them on or off");
                    if ui.button("Switch all on").clicked() {
                        self.prefs.wh_off_holes.clear();
                        self.prefs.wh_off_systems.clear();
                        changed = true;
                    }
                });
            } else {
                ui.label(egui::RichText::new("Right-click a system with holes to keep them off routes.").weak());
            }
        });
        if changed {
            self.dirty = true;
            self.replan();
        }
    }

    /// "Set in game" for a character that granted the waypoint scope.
    fn set_in_game(&mut self, ui: &mut egui::Ui, o: &RouteOption) {
        let able: Vec<&Pilot> = self.pilots.iter().filter(|p| p.waypoints).collect();
        if able.is_empty() {
            return;
        }
        let pilot = self.pilot.filter(|id| able.iter().any(|p| p.char_id == *id)).unwrap_or(able[0].char_id);
        let mut chosen = pilot;
        ui.horizontal(|ui| {
            if able.len() > 1 {
                let name = able.iter().find(|p| p.char_id == chosen).map(|p| p.name.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt("web_route_pilot").selected_text(name).show_ui(ui, |ui| {
                    for p in &able {
                        ui.selectable_value(&mut chosen, p.char_id, &p.name);
                    }
                });
            }
            let who = able.iter().find(|p| p.char_id == chosen);
            let label = if able.len() > 1 { "Set in game".to_owned() } else { format!("Set in game for {}", who.map_or("", |p| p.name.as_str())) };
            if ui
                .button(format!("{}  {label}", icon::MAP_PIN_LINE))
                .on_hover_text(if self.kind == "gate" { "One waypoint per system" } else { "Waypoints at both ends of each leg you fly yourself" })
                .clicked()
            {
                let path = route::ingame_waypoints(o, who.and_then(|p| p.system));
                self.request = Some(PlanRequest::SetRoute(chosen, path));
            }
        });
        self.pilot = Some(chosen);
        match &self.note {
            Some(Ok(t)) => {
                ui.label(egui::RichText::new(t).color(spai_ui::theme::standing::FRIENDLY));
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).color(spai_ui::theme::standing::WARNING));
            }
            None => {}
        }
    }

    /// Jump skills read from a character.
    pub fn set_skills(&mut self, jdc: u32, jfc: u32) {
        self.prefs.jdc = jdc.min(5);
        self.prefs.jfc = jfc.min(5);
        self.dirty = true;
        self.replan();
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
    fn a_destination_alone_routes_from_the_added_character() {
        let mut p = RoutePlan::default();
        p.pilots = vec![Pilot { char_id: 1, name: "A".into(), system: Some(30000142), waypoints: true, skills: false }];
        p.set_dest(30002187, Some("gate"));
        assert_eq!(p.anchors, vec![30000142, 30002187]);
        assert_eq!(p.leg_kinds, vec!["gate"]);
        let mut alone = RoutePlan::default();
        alone.set_dest(30002187, None);
        assert_eq!(alone.anchors, vec![30002187]);
    }

    #[test]
    fn a_route_can_skip_the_groups_ansiblexes() {
        let mut geo = spai_core::test_support::small_universe(&[]);
        let network = crate::starmap::Network { capital: "1DQ1-A".into(), max_zone: 5, bridges: vec![("1DQ1-A".into(), "7-K5EL".into())] };
        spai_core::ansiblex::feed(spai_core::ansiblex::BridgeKey { bridges: network.list(), capital: network.capital.clone(), max_zone: 5 }, &mut geo);
        let (from, to) = (geo.lookup("1DQ1-A").unwrap().id, geo.lookup("7-K5EL").unwrap().id);
        let coords: Vec<MapSystem> = Vec::new();
        let input = PlanInput { geo: &geo, coords: &coords, holes: &[], network: &network };
        let mut p = RoutePlan::default();
        p.take("gate", "gate", from, to);
        p.update(&input);
        assert_eq!(p.route().unwrap().path.len(), 2, "over the bridge");
        p.zone = Some(0);
        p.replan();
        p.update(&input);
        assert_eq!(p.route().unwrap().path.len(), 3, "by gates only");
        p.zone = None;
        p.replan();
        p.update(&input);
        assert_eq!(p.route().unwrap().path.len(), 2, "the group's limit again");
    }

    #[test]
    fn a_drag_plans_and_a_drag_off_the_destination_extends_it() {
        let geo = spai_core::test_support::small_universe(&[]);
        let coords: Vec<MapSystem> = Vec::new();
        let network = crate::starmap::Network::default();
        let input = PlanInput { geo: &geo, coords: &coords, holes: &[], network: &network };
        let mut p = RoutePlan::default();
        p.take("gate", "gate", 30_004_759, 30_004_608);
        p.update(&input);
        assert_eq!(p.route().map(|o| o.path.clone()), Some(vec![30_004_759, 30_004_608]));
        p.take("gate", "gate", 30_004_608, 30_003_704);
        p.update(&input);
        assert_eq!(p.anchors, vec![30_004_759, 30_004_608, 30_003_704], "the destination became a waypoint");
        assert_eq!(p.route().unwrap().path.len(), 3);
        p.take("gate", "gate", 30_003_704, 30_004_759);
        assert_eq!(p.anchors.len(), 4);
        p.take("jump", "jump", 30_000_142, 30_004_759);
        assert_eq!((p.kind, p.anchors.clone()), ("jump", vec![30_000_142, 30_004_759]), "from elsewhere: a new route");
    }

    #[test]
    fn a_mixed_route_keeps_each_legs_kind() {
        let mut p = RoutePlan::default();
        p.take("mixed", "jump", 1, 2);
        p.take("mixed", "gate", 2, 3);
        assert_eq!((p.kind, p.leg_kinds.clone()), ("mixed", vec!["jump", "gate"]));
        p.add_waypoint(4, Some("jump"));
        assert_eq!((p.anchors.clone(), p.leg_kinds.clone()), (vec![1, 2, 4, 3], vec!["jump", "jump", "gate"]));
        p.remove_anchor(1);
        assert_eq!(p.leg_kinds, vec!["jump", "gate"]);
        let mut g = RoutePlan::default();
        g.take("gate", "gate", 1, 2);
        g.set_leg_kind(0, "jump");
        assert_eq!(g.kind, "mixed", "a plain route with a leg switched is mixed");
        g.set_mode("gate");
        assert_eq!(g.leg_kinds, vec!["gate"]);
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

    #[test]
    fn switched_off_and_filtered_holes_stay_off_routes() {
        let geo = spai_core::test_support::small_universe(&[]);
        let hole = |uid: &str, a: i64, b: i64| Wormhole { uid: uid.into(), system_id: a, dest_system_id: Some(b), reported_at: spai_core::clock::utc().timestamp(), ..Default::default() };
        let holes = [hole("h1", 30_004_759, 30_000_142), hole("h2", 30_004_608, 30_002_187)];
        let now = spai_core::clock::utc().timestamp();
        let mut p = PlanPrefs::default();
        assert_eq!(p.hole_graph(&geo, &holes, now).len(), 4, "both holes, both ways");
        p.wh_off_holes.push("h1".into());
        assert!(!p.hole_graph(&geo, &holes, now).contains_key(&30_004_759), "switched off");
        p.wh_off_systems.push(30_002_187);
        assert!(p.hole_graph(&geo, &holes, now).is_empty(), "a system switched off takes its holes");
        let mut none = PlanPrefs::default();
        none.via_holes = false;
        assert!(none.hole_graph(&geo, &holes, now).is_empty());
        let old: Vec<PlanPrefs> = serde_json::from_str(r#"[{"avoid_gate":[1],"avoid_jump":[],"ship":0,"jdc":5,"jfc":5,"via_holes":true}]"#).unwrap();
        assert_eq!(old[0].wh_kinds.len(), HoleKind::ALL.len(), "saved prefs from before keep every kind");
    }
}
