//! The fight in 3D: the two fleets and any bubbles over a grid, moved like probes. Dragging a
//! fleet, a bubble or an arrow's tip moves it on the level (Shift raises or lowers it); an arrow
//! steers its fleet in 3D and sets its speed; dragging empty space turns the view, scrolling zooms.
//! Right-dragging pans over the level. The world stays put: the view turns about its own point.
//! Placing the fleets sets the range, the fleet's heading and the opponent's motion.

use egui::{Color32, Pos2, Stroke};
use spai_fit::fleet::scene::{add, cross, dot, len, norm, scale, sub, Bubble, Scene, SceneObject, V3};
use spai_fit::fleet::{HullClass, Formation};
use spai_ui::{tr, trf};

use crate::plot::{nice_step, series_color};
use crate::{text, SimUi};

/// The view: turned about a point of the world, from above or below the level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneCam {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: V3,
}

impl Default for SceneCam {
    fn default() -> Self {
        SceneCam { yaw: 0.6, pitch: 0.55, dist: 90_000.0, target: [0.0; 3] }
    }
}

/// Ships drawn per fleet at most; a bigger fleet is drawn in proportion.
const MAX_SHIPS: usize = 300;

/// A fleet's ships by hull class, one entry a ship, in proportion when it is big.
fn hulls(slots: &[(HullClass, u32)]) -> Vec<HullClass> {
    let total: u32 = slots.iter().map(|(_, n)| n).sum();
    let k = if total as usize > MAX_SHIPS { MAX_SHIPS as f64 / f64::from(total) } else { 1.0 };
    slots.iter().flat_map(|(c, n)| std::iter::repeat_n(*c, (f64::from(*n) * k).round().max(if *n > 0 { 1.0 } else { 0.0 }) as usize)).collect()
}

/// A hull-size symbol in the overview's manner: a triangle growing with the class, a bar under
/// the in-between classes, a double outline for capitals, a square for haulers.
fn symbol(painter: &egui::Painter, c: Pos2, class: HullClass, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    let tri = |s: f32| vec![c + egui::vec2(0.0, -s), c + egui::vec2(s * 0.95, s * 0.7), c + egui::vec2(-s * 0.95, s * 0.7)];
    let (size, bar) = match class {
        HullClass::Frigate => (3.5, false),
        HullClass::Destroyer => (3.5, true),
        HullClass::Cruiser => (5.0, false),
        HullClass::Battlecruiser => (5.0, true),
        HullClass::Battleship => (6.5, false),
        HullClass::Capital => (8.0, false),
        HullClass::Industrial => {
            painter.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(7.0, 7.0)), 0.0, stroke, egui::StrokeKind::Middle);
            return;
        }
    };
    painter.add(egui::Shape::closed_line(tri(size), stroke));
    if class == HullClass::Capital {
        painter.add(egui::Shape::closed_line(tri(size + 3.0), Stroke::new(1.0, color.gamma_multiply(0.7))));
    }
    if bar {
        let y = c.y + size * 0.7 + 2.5;
        painter.line_segment([Pos2::new(c.x - size, y), Pos2::new(c.x + size, y)], stroke);
    }
}

/// What a drag holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grab {
    View,
    Own,
    Enemy,
    OwnTip,
    EnemyTip,
    Object(usize),
}

pub fn bubble_name(b: Bubble) -> &'static str {
    match b {
        Bubble::AnchoredSmallI => tr!("Mobile Small Warp Disruptor I"),
        Bubble::AnchoredSmallII => tr!("Mobile Small Warp Disruptor II"),
        Bubble::AnchoredMediumI => tr!("Mobile Medium Warp Disruptor I"),
        Bubble::AnchoredMediumII => tr!("Mobile Medium Warp Disruptor II"),
        Bubble::AnchoredLargeI => tr!("Mobile Large Warp Disruptor I"),
        Bubble::AnchoredLargeII => tr!("Mobile Large Warp Disruptor II"),
        Bubble::InterdictorProbe => tr!("Interdictor bubble"),
        Bubble::SurgicalProbe => tr!("Surgical interdictor bubble"),
        Bubble::HeavyInterdictor => tr!("Heavy interdictor bubble"),
    }
}

/// A perspective camera over metres, y up.
struct Cam {
    eye: V3,
    fwd: V3,
    right: V3,
    up: V3,
    f: f32,
    center: Pos2,
}

impl Cam {
    fn new(c: SceneCam, target: V3, rect: egui::Rect) -> Cam {
        let off = [c.pitch.cos() * c.yaw.sin(), c.pitch.sin(), c.pitch.cos() * c.yaw.cos()];
        let eye = add(target, scale(off, c.dist));
        let fwd = norm(sub(target, eye));
        let right = norm(cross(fwd, [0.0, 1.0, 0.0]));
        let up = cross(right, fwd);
        // About 60° across the shorter side.
        let f = rect.width().min(rect.height()) / (2.0 * 30f32.to_radians().tan());
        Cam { eye, fwd, right, up, f, center: rect.center() }
    }

    /// Where a point lands on screen and how far ahead it is; none behind the eye.
    fn project(&self, p: V3) -> Option<(Pos2, f32)> {
        let d = sub(p, self.eye);
        let z = dot(d, self.fwd);
        if z < 1.0 {
            return None;
        }
        Some((Pos2::new(self.center.x + dot(d, self.right) * self.f / z, self.center.y - dot(d, self.up) * self.f / z), z))
    }

    /// Where the ray through a screen point meets the level at height `y`.
    fn on_level(&self, s: Pos2, y: f32) -> Option<V3> {
        let dir = norm(add(self.fwd, add(scale(self.right, (s.x - self.center.x) / self.f), scale(self.up, -(s.y - self.center.y) / self.f))));
        if dir[1].abs() < 1e-5 {
            return None;
        }
        let t = (y - self.eye[1]) / dir[1];
        (t > 0.0).then(|| add(self.eye, scale(dir, t)))
    }
}

fn formation_r(f: Formation) -> f32 {
    match f {
        Formation::Ball { radius_m } => radius_m,
        Formation::Flock { sigma_m } => 2.0 * sigma_m,
    }
}

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui) {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| body(sim, ui));
}

fn body(sim: &mut SimUi, ui: &mut egui::Ui) {
    let enemy = sim.fleet.as_ref().and_then(|f| f.fleet.enemy.clone()).and_then(|id| sim.enemy_fleet(&id));
    // Each side's hulls and slowest top speed, for the symbols and the arrows.
    let summary = |d: &spai_fit::Dogma, s: &spai_fit::fleet::FleetStats| (s.slots.iter().map(|x| (spai_fit::fleet::hull_class(d, x.ship), x.count)).collect::<Vec<_>>(), s.speed_min);
    let dogma = sim.dogma.clone();
    let sides: [(Vec<(HullClass, u32)>, f64); 2] = match (dogma.as_ref(), sim.fleet_analysis()) {
        (Some(d), Some(an)) => [summary(d, &an.mine), summary(d, &an.theirs)],
        (Some(d), None) => [sim.fleet_stats().map_or((Vec::new(), 0.0), |(m, _)| summary(d, &m)), (Vec::new(), 0.0)],
        _ => [(Vec::new(), 0.0), (Vec::new(), 0.0)],
    };
    let Some(session) = sim.fleet.as_mut() else {
        ui.label(egui::RichText::new(tr!("No fleet open.")).weak());
        return;
    };
    let f = &mut session.fleet;
    let mut scene = f.scene.clone().unwrap_or_else(|| {
        let mut s = Scene::default();
        s.follow(f.range, f.motion.1, enemy.as_ref().map(|e| e.motion));
        s
    });
    scene.follow(f.range, f.motion.1, None);
    let before = scene.clone();
    let motion_before = f.motion;
    let mut reset = false;
    ui.horizontal_wrapped(|ui| {
        ui.menu_button(trf!("{icon} Add a bubble", icon = egui_phosphor::regular::PLUS), |ui| {
            ui.set_min_width(260.0);
            ui.label(egui::RichText::new(tr!("Launched")).weak());
            for b in Bubble::ALL.iter().filter(|b| !b.anchored()) {
                if ui.button(format!("{}  ({})", bubble_name(*b), text::metres(f64::from(b.radius())))).clicked() {
                    let mid = scale(add(scene.own_at, scene.enemy_at), 0.5);
                    scene.objects.push(SceneObject { bubble: *b, at: mid });
                    ui.close();
                }
            }
            ui.separator();
            ui.label(egui::RichText::new(tr!("Anchored")).weak());
            for b in Bubble::ALL.iter().filter(|b| b.anchored()) {
                if ui.button(format!("{}  ({})", bubble_name(*b), text::metres(f64::from(b.radius())))).clicked() {
                    let mid = scale(add(scene.own_at, scene.enemy_at), 0.5);
                    scene.objects.push(SceneObject { bubble: *b, at: mid });
                    ui.close();
                }
            }
        });
        if ui.button(tr!("Reset view")).clicked() {
            reset = true;
        }
        ui.add(egui::Label::new(egui::RichText::new(tr!("Drag a fleet, bubble or arrow tip to move it, with Shift up and down; an arrow sets heading and speed; drag space to turn, right-drag to pan, scroll to zoom; right-click a bubble to remove it.")).weak()).wrap());
    });
    // A first look, or a reset, frames both fleets; after that the view stays where it is put.
    let framed = SceneCam { dist: (scene.range() * 2.2).max(20_000.0), target: scale(add(scene.own_at, scene.enemy_at), 0.5), ..SceneCam::default() };
    if reset {
        ui.ctx().data_mut(|d| d.insert_temp(cam_id(), framed));
    }
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), (ui.available_height() - 4.0).max(240.0)), egui::Sense::click_and_drag());
    let mut cam_state: SceneCam = ui.ctx().data(|d| d.get_temp(cam_id())).unwrap_or(framed);
    let target = cam_state.target;
    let cam = Cam::new(cam_state, target, rect);
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    painter.rect_filled(rect, 4.0, visuals.extreme_bg_color);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let (own_c, enemy_c) = (series_color(0), series_color(1));
    let arrow_max = (cam_state.dist * 0.25).max(2_000.0);

    // The level at the lowest object, a grid around the middle.
    let half = cam_state.dist.max(scene.range() * 0.75);
    let step = nice_step(f64::from(half * 2.0), 10) as f32;
    let grid = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color.gamma_multiply(0.7));
    let n = (half / step).ceil() as i32;
    // On the world's own lines, so the grid never slides when the view moves.
    let (gx, gz) = ((target[0] / step).round() * step, (target[2] / step).round() * step);
    for i in -n..=n {
        let k = i as f32 * step;
        for (a, b) in [([gx + k, 0.0, gz - half], [gx + k, 0.0, gz + half]), ([gx - half, 0.0, gz + k], [gx + half, 0.0, gz + k])] {
            if let (Some((pa, _)), Some((pb, _))) = (cam.project(a), cam.project(b)) {
                painter.line_segment([pa, pb], grid);
            }
        }
    }
    let stalk = |p: V3, color: Color32| {
        if let (Some((top, _)), Some((foot, _))) = (cam.project(p), cam.project([p[0], 0.0, p[2]])) {
            painter.line_segment([top, foot], Stroke::new(1.0, color.gamma_multiply(0.5)));
            painter.circle_stroke(foot, 3.0, Stroke::new(1.0, color.gamma_multiply(0.5)));
        }
    };
    // Bubbles first, behind the fleets.
    for o in &scene.objects {
        let Some((c, z)) = cam.project(o.at) else { continue };
        let r = o.bubble.radius() * cam.f / z;
        let color = if o.bubble.anchored() { Color32::from_rgb(200, 120, 220) } else { Color32::from_rgb(220, 200, 90) };
        painter.circle(c, r, color.gamma_multiply(0.10), Stroke::new(1.0, color.gamma_multiply(0.7)));
        stalk(o.at, color);
        let own_d = (len(sub(o.at, scene.own_at)) - o.bubble.radius()).max(0.0);
        let label = if own_d <= 0.0 { trf!("{b}, around the fleet", b = bubble_name(o.bubble)) } else { trf!("{b}, edge {d} from the fleet", b = bubble_name(o.bubble), d = text::metres(f64::from(own_d))) };
        painter.text(c + egui::vec2(0.0, -r.min(rect.height() / 2.0) - 4.0), egui::Align2::CENTER_BOTTOM, label, font.clone(), color);
    }
    // The fleets: their spread as a sphere, a stalk to the level, the heading arrow.
    let enemy_r = enemy.as_ref().map_or(5_000.0, |e| formation_r(e.formation));
    let enemy_form = enemy.as_ref().map_or(Formation::default(), |e| e.formation);
    let fleets = [(scene.own_at, scene.own_dir, f.motion.0, formation_r(f.formation), own_c, f.name.clone()), (scene.enemy_at, scene.enemy_dir, scene.enemy_share, enemy_r, enemy_c, enemy.as_ref().map_or_else(|| tr!("Opponent").to_owned(), |e| e.name.clone()))];
    // Every ship by its hull symbol where its formation puts it.
    for (i, form) in [f.formation, enemy_form].into_iter().enumerate() {
        let list = hulls(&sides[i].0);
        let at = fleets[i].0;
        for (class, pos) in list.iter().zip(spai_fit::fleet::geometry::positions(form, list.len())) {
            if let Some((c, _)) = cam.project(add(at, [pos[0] as f32, pos[1] as f32, pos[2] as f32])) {
                symbol(&painter, c, *class, fleets[i].4);
            }
        }
    }
    let mut tips = [Pos2::ZERO; 2];
    let mut anchors = [Pos2::ZERO; 2];
    for (i, (at, dir, share, r, color, name)) in fleets.iter().enumerate() {
        stalk(*at, *color);
        let Some((c, z)) = cam.project(*at) else { continue };
        anchors[i] = c;
        painter.circle(c, (r * cam.f / z).max(4.0), color.gamma_multiply(0.05), Stroke::new(1.0, color.gamma_multiply(0.45)));
        painter.circle_stroke(c, 7.0, Stroke::new(2.0, *color));
        // A still fleet keeps a short stub to grab, drawn dashed.
        let still = *share < 0.05;
        let tip3 = add(*at, scale(norm(*dir), share.clamp(0.1, 1.0) * arrow_max));
        if let Some((tip, _)) = cam.project(tip3) {
            tips[i] = tip;
            if still {
                painter.add(egui::Shape::dashed_line(&[c, tip], Stroke::new(1.5, color.gamma_multiply(0.7)), 4.0, 3.0));
            } else {
                painter.arrow(c, tip - c, Stroke::new(2.0, *color));
            }
            painter.circle_stroke(tip, 6.0, Stroke::new(1.5, *color));
            let speed = f64::from(share.clamp(0.0, 1.0)) * sides[i].1;
            let label = if sides[i].1 > 0.0 { format!("{:.0}% · {} m/s", share * 100.0, text::num(speed)) } else { format!("{:.0}%", share * 100.0) };
            boxed(&painter, tip + egui::vec2(9.0, 0.0), egui::Align2::LEFT_CENTER, label, &font, *color, visuals.extreme_bg_color);
        }
        boxed(&painter, c + egui::vec2(10.0, -10.0), egui::Align2::LEFT_BOTTOM, name.clone(), &font, *color, visuals.extreme_bg_color);
    }
    // The distance between them.
    if anchors[0] != Pos2::ZERO && anchors[1] != Pos2::ZERO {
        painter.add(egui::Shape::dashed_line(&[anchors[0], anchors[1]], Stroke::new(1.0, visuals.text_color()), 6.0, 4.0));
        let mid = anchors[0] + (anchors[1] - anchors[0]) * 0.6;
        let galley = painter.layout_no_wrap(text::metres(f64::from(scene.range())), font.clone(), visuals.text_color());
        let at = mid - galley.size() / 2.0;
        painter.rect_filled(egui::Rect::from_min_size(at, galley.size()).expand(3.0), 3.0, visuals.extreme_bg_color);
        painter.galley(at, galley, visuals.text_color());
    }

    // Picking: arrow tips first, then the fleets, then bubbles by their centre.
    let object_pts: Vec<Option<Pos2>> = scene.objects.iter().map(|o| cam.project(o.at).map(|(c, _)| c)).collect();
    // The nearest handle within reach, else the view.
    let pick = |p: Pos2| -> Grab {
        let mut handles: Vec<(Grab, Pos2)> = vec![(Grab::OwnTip, tips[0]), (Grab::EnemyTip, tips[1]), (Grab::Own, anchors[0]), (Grab::Enemy, anchors[1])];
        handles.extend(object_pts.iter().enumerate().filter_map(|(i, c)| c.map(|c| (Grab::Object(i), c))));
        handles.into_iter().filter(|(_, q)| *q != Pos2::ZERO && q.distance(p) < 14.0).min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p))).map_or(Grab::View, |(g, _)| g)
    };
    let grab_id = egui::Id::new("sim_scene_grab");
    if response.drag_started_by(egui::PointerButton::Primary) {
        if let Some(p) = response.interact_pointer_pos() {
            let g = pick(p);
            ui.ctx().data_mut(|d| d.insert_temp(grab_id, g));
        }
    }
    let grab: Option<Grab> = ui.ctx().data(|d| d.get_temp(grab_id));
    if response.dragged_by(egui::PointerButton::Primary) {
        let delta = response.drag_delta();
        let shift = ui.input(|i| i.modifiers.shift);
        let pointer = response.interact_pointer_pos();
        let moved = |at: &mut V3| {
            let Some(p) = pointer else { return };
            if shift {
                let z = cam.project(*at).map_or(cam_state.dist, |(_, z)| z);
                at[1] -= delta.y * z / cam.f;
            } else if let Some(hit) = cam.on_level(p, at[1]) {
                *at = hit;
            }
        };
        match grab.unwrap_or(Grab::View) {
            Grab::View => {
                cam_state.yaw -= delta.x * 0.008;
                // Below the level too, to look up at the scene.
                cam_state.pitch = (cam_state.pitch + delta.y * 0.008).clamp(-1.5, 1.5);
            }
            Grab::Own => moved(&mut scene.own_at),
            Grab::Enemy => moved(&mut scene.enemy_at),
            Grab::Object(i) => {
                if let Some(o) = scene.objects.get_mut(i) {
                    moved(&mut o.at);
                }
            }
            Grab::OwnTip | Grab::EnemyTip => {
                // The tip moves like a fleet: on the level at its own height, or with Shift up
                // and down, so the arrow points anywhere in 3D.
                let own = grab == Some(Grab::OwnTip);
                let (at, dir, share) = if own { (scene.own_at, scene.own_dir, f.motion.0) } else { (scene.enemy_at, scene.enemy_dir, scene.enemy_share) };
                let mut tip = add(at, scale(norm(dir), share.clamp(0.1, 1.0) * arrow_max));
                moved(&mut tip);
                let v = sub(tip, at);
                if len(v) > 1.0 {
                    let share = (len(v) / arrow_max).clamp(0.0, 1.0);
                    if own {
                        scene.own_dir = norm(v);
                        f.motion.0 = share;
                    } else {
                        scene.enemy_dir = norm(v);
                        scene.enemy_share = share;
                    }
                }
            }
        }
    }
    // A right drag pans the view over the level.
    if response.dragged_by(egui::PointerButton::Secondary) {
        if let Some(p) = response.interact_pointer_pos() {
            if let (Some(now), Some(was)) = (cam.on_level(p, cam_state.target[1]), cam.on_level(p - response.drag_delta(), cam_state.target[1])) {
                cam_state.target = add(cam_state.target, sub(was, now));
            }
        }
    }
    if response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<Grab>(grab_id));
    }
    if response.secondary_clicked() {
        if let Some(Grab::Object(i)) = response.interact_pointer_pos().map(pick) {
            scene.objects.remove(i);
        }
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            cam_state.dist = (cam_state.dist * (-scroll * 0.002).exp()).clamp(2_000.0, 2_000_000.0);
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(cam_id(), cam_state));
    if scene != before || f.scene.is_none() || f.motion != motion_before {
        let (range, own_heading, _) = scene.read();
        let changed = scene != before || f.motion != motion_before;
        f.range = range;
        f.motion.1 = own_heading;
        f.scene = Some(scene);
        if changed {
            session.dirty = true;
        }
    }
}

fn cam_id() -> egui::Id {
    egui::Id::new("sim_scene_cam")
}

/// Text on a backdrop, readable over the ship symbols.
fn boxed(painter: &egui::Painter, at: Pos2, align: egui::Align2, text: String, font: &egui::FontId, color: Color32, bg: Color32) {
    let galley = painter.layout_no_wrap(text, font.clone(), color);
    let rect = align.anchor_size(at, galley.size());
    painter.rect_filled(rect.expand(2.0), 2.0, bg.gamma_multiply(0.85));
    painter.galley(rect.min, galley, color);
}
