//! The Probes tab: custom probe formations, edited in a 3D view and as numbers.

use super::*;
use crate::evesettings::probes::{self, Formation, Probe, Shape, AU, KM, MAX_PROBES, RANGES_AU};

/// Perches sit on grid, which ends about here.
const GRID_KM: f64 = 30_000.0;
const PICK_PX: f32 = 11.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Formations for scanning: offsets and ranges in AU.
    Scan,
    /// Formations that park probes on grid as warp-in points: offsets in km.
    Perch,
}

impl Mode {
    fn unit(self) -> f64 {
        match self {
            Mode::Scan => AU,
            Mode::Perch => KM,
        }
    }

    fn snap(self) -> f64 {
        match self {
            Mode::Scan => 0.25 * AU,
            Mode::Perch => 100.0 * KM,
        }
    }

    /// A formation whose probes all sit within a fraction of an AU is a perch formation.
    fn guess(f: &Formation) -> Mode {
        if !f.probes.is_empty() && f.extent() <= 5.0 * GRID_KM * KM {
            Mode::Perch
        } else {
            Mode::Scan
        }
    }
}

pub(crate) struct State {
    pub(crate) selected: Option<i64>,
    pub(crate) probe: Option<usize>,
    pub(crate) mode: Option<Mode>,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    dragging: Option<usize>,
    snap: bool,
    shape: Shape,
    count: usize,
    /// Radius for the shape generator, in the mode's unit.
    radius: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            selected: None,
            probe: None,
            mode: None,
            yaw: 0.6,
            pitch: 0.5,
            zoom: 1.0,
            dragging: None,
            snap: true,
            shape: Shape::Pinpoint,
            count: 7,
            radius: 0.0,
        }
    }
}

/// An orbit camera looking at the formation's centre, without perspective, so a drag maps to the
/// same distance wherever the probe is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Camera {
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    /// Pixels per metre.
    pub(crate) scale: f64,
    pub(crate) center: egui::Pos2,
}

impl Camera {
    /// Screen position and depth (larger is nearer the viewer).
    pub(crate) fn project(&self, p: [f64; 3]) -> (egui::Pos2, f64) {
        let (sy, cy) = (f64::from(self.yaw).sin(), f64::from(self.yaw).cos());
        let (sp, cp) = (f64::from(self.pitch).sin(), f64::from(self.pitch).cos());
        let x1 = p[0] * cy + p[2] * sy;
        let z1 = -p[0] * sy + p[2] * cy;
        let y2 = p[1] * cp - z1 * sp;
        // The viewer sits above the plane on the +z side once the yaw is taken out.
        let depth = p[1] * sp + z1 * cp;
        (egui::pos2(self.center.x + (x1 * self.scale) as f32, self.center.y - (y2 * self.scale) as f32), depth)
    }

    /// The point under `at` on the horizontal plane at height `y`. `None` when the camera looks
    /// along that plane and the answer runs off to infinity.
    pub(crate) fn on_plane(&self, at: egui::Pos2, y: f64) -> Option<[f64; 3]> {
        let (sy, cy) = (f64::from(self.yaw).sin(), f64::from(self.yaw).cos());
        let (sp, cp) = (f64::from(self.pitch).sin(), f64::from(self.pitch).cos());
        if sp.abs() < 0.05 {
            return None;
        }
        let x1 = f64::from(at.x - self.center.x) / self.scale;
        let y2 = -f64::from(at.y - self.center.y) / self.scale;
        let z1 = (y * cp - y2) / sp;
        Some([x1 * cy - z1 * sy, y, x1 * sy + z1 * cy])
    }

    /// The height a vertical screen movement of `dy` pixels corresponds to.
    pub(crate) fn lift(&self, dy: f32) -> f64 {
        -f64::from(dy) / self.scale / f64::from(self.pitch).cos().max(0.2)
    }
}

/// The probe nearest `at` on screen, within reach; the nearest to the viewer on a tie.
pub(crate) fn pick(cam: &Camera, probes: &[Probe], at: egui::Pos2, reach: f32) -> Option<usize> {
    probes
        .iter()
        .enumerate()
        .map(|(i, p)| (i, cam.project(p.pos)))
        .filter(|(_, (pos, _))| pos.distance(at) <= reach)
        .max_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
        .map(|(i, _)| i)
}

fn snapped(v: f64, step: f64) -> f64 {
    (v / step).round() * step
}

/// Round distances for the ground rings: 1, 2 or 5 times a power of ten, about four of them
/// inside `radius`.
pub(crate) fn ring_step(radius: f64) -> f64 {
    let raw = radius / 4.0;
    let pow = 10f64.powf(raw.log10().floor());
    let lead = raw / pow;
    pow * if lead < 1.5 { 1.0 } else if lead < 3.5 { 2.0 } else if lead < 7.5 { 5.0 } else { 10.0 }
}

fn grouped(n: f64) -> String {
    let digits = format!("{:.0}", n.abs());
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0.0 {
        out.insert(0, '-');
    }
    out
}

pub(crate) fn distance_text(metres: f64, mode: Mode) -> String {
    match mode {
        Mode::Perch => format!("{} km", grouped(metres / KM)),
        Mode::Scan => {
            let au = metres / AU;
            let text = format!("{au:.2}");
            format!("{} AU", text.trim_end_matches('0').trim_end_matches('.'))
        }
    }
}

fn range_text(range: f64) -> String {
    distance_text(range, Mode::Scan)
}

fn view_radius(f: &Formation, mode: Mode) -> f64 {
    match mode {
        Mode::Perch => (f.extent() * 1.25).max(1_000.0 * KM),
        Mode::Scan => {
            let reach = f.probes.iter().map(|p| probes::length(p.pos) + p.range).fold(0.0, f64::max);
            (reach * 1.1).max(2.0 * AU)
        }
    }
}

fn new_formation(all: &probes::Formations, mode: Mode) -> Formation {
    let (shape, count, radius, range, name) = match mode {
        Mode::Scan => (Shape::Pinpoint, 7, 2.0 * AU, 4.0 * AU, tr!("New formation")),
        Mode::Perch => (Shape::Ring, 8, 5_000.0 * KM, 0.25 * AU, tr!("New perches")),
    };
    Formation { id: all.next_id(), name: name.to_owned(), probes: probes::generate(shape, count, radius, range) }
}

pub(crate) fn ui(ui: &mut egui::Ui, st: &mut State, s: &mut Session) {
    let shared = s.account_id.is_some();
    let all = &mut s.probes.now;
    if !st.selected.is_some_and(|id| all.list.iter().any(|f| f.id == id)) {
        st.selected = all.list.first().map(|f| f.id);
        st.probe = None;
        st.mode = None;
    }
    let at = st.selected.and_then(|id| all.list.iter().position(|f| f.id == id));
    let mode = st.mode.or_else(|| at.map(|i| Mode::guess(&all.list[i]))).unwrap_or(Mode::Scan);

    ui.add_space(4.0);
    let wide = ui.available_width() >= 190.0 + TABLE_W + 260.0;
    ui.horizontal_wrapped(|ui| {
        for (m, label, hover) in [
            (Mode::Scan, tr!("Scanning"), tr!("Formations for scanning down signatures and ships, measured in AU")),
            (Mode::Perch, tr!("Perches"), tr!("Formations that park probes on grid, measured in km")),
        ] {
            if ui.menu_label(mode == m, label).on_hover_text(hover).clicked() {
                st.mode = Some(m);
                st.zoom = 1.0;
            }
        }
        ui.separator();
        if !wide {
            let current = at.map_or_else(|| tr!("No formations").to_owned(), |i| all.list[i].name.clone());
            egui::ComboBox::from_id_salt("probe_formation").selected_text(current).width(160.0).truncate().show_ui(ui, |ui| {
                for f in &all.list {
                    if ui.menu_label(st.selected == Some(f.id), f.name.as_str()).clicked() {
                        st.selected = Some(f.id);
                        st.probe = None;
                        st.mode = None;
                    }
                }
            });
        }
        if ui.button(trf!("{icon}  New", icon = icon::PLUS)).on_hover_text(tr!("A new formation for the mode shown")).clicked() {
            let f = new_formation(all, mode);
            st.selected = Some(f.id);
            st.mode = Some(mode);
            st.probe = None;
            all.list.push(f);
        }
        // Plain widgets, not a nested ui: this row wraps.
        if ui.add_enabled(at.is_some(), egui::Button::new(trf!("{icon}  Duplicate", icon = icon::COPY))).clicked() {
            if let Some(i) = at {
                let mut f = all.list[i].clone();
                f.id = all.next_id();
                f.name = trf!("{name} copy", name = f.name);
                st.selected = Some(f.id);
                all.list.push(f);
            }
        }
        if ui.add_enabled(at.is_some(), egui::Button::new(trf!("{icon}  Delete", icon = icon::TRASH))).clicked() {
            if let Some(i) = at {
                all.list.remove(i);
                st.selected = None;
            }
        }
        ui.separator();
        ui.checkbox(&mut st.snap, tr!("Snap")).on_hover_text(trf!("Dragged probes move in steps of {step}. Hold Alt to move freely.", step = distance_text(mode.snap(), mode)));
        if ui.button(icon::ARROWS_OUT_CARDINAL).on_hover_text(tr!("Reset the view")).clicked() {
            let fresh = State::default();
            (st.yaw, st.pitch, st.zoom) = (fresh.yaw, fresh.pitch, fresh.zoom);
        }
        if shared {
            let hint = tr!("Stored per account: every character on the account shares these.");
            clipped(ui, egui::RichText::new(hint).weak(), hint);
        }
    });
    ui.add_space(4.0);

    if wide {
        egui::Panel::left("probe_formations").resizable(false).exact_size(190.0).frame(egui::Frame::NONE.inner_margin(egui::Margin { right: 8, ..Default::default() })).show_inside(ui, |ui| {
            super::sidebar_tab::scroll(ui, "probe_formation_rows", |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                if all.list.is_empty() {
                    ui.label(egui::RichText::new(tr!("No custom formations saved.")).weak());
                }
                for f in &all.list {
                    let glyph = if Mode::guess(f) == Mode::Perch { icon::MAP_PIN } else { icon::CROSSHAIR };
                    let r = person_row(ui, st.selected == Some(f.id), None, Some(glyph), &f.name, None);
                    if r.on_hover_text(trf!("{name}\nProbes: {n}", name = f.name, n = f.probes.len())).clicked() {
                        st.selected = Some(f.id);
                        st.probe = None;
                        st.mode = None;
                    }
                }
            });
        });
    }
    let Some(f) = st.selected.and_then(|id| all.list.iter_mut().find(|f| f.id == id)) else {
        if !wide {
            ui.label(egui::RichText::new(tr!("No custom formations saved.")).weak());
        }
        return;
    };
    if st.probe.is_some_and(|i| i >= f.probes.len()) {
        st.probe = None;
    }
    if wide {
        egui::Panel::right("probe_table").resizable(false).exact_size(TABLE_W + 8.0).frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 8, ..Default::default() })).show_inside(ui, |ui| table(ui, st, f, mode, true));
        scene(ui, st, f, mode, ui.available_rect_before_wrap());
    } else {
        // The view takes what the table can spare, and the table scrolls as a whole when even
        // that is too little.
        let avail = ui.available_rect_before_wrap();
        let height = (avail.height() - 250.0).clamp(160.0, 420.0).min(avail.height());
        scene(ui, st, f, mode, egui::Rect::from_min_size(avail.min, egui::vec2(avail.width(), height)));
        ui.add_space(6.0);
        super::sidebar_tab::scroll(ui, "probe_table_scroll", |ui| table(ui, st, f, mode, false));
    }
}

const NUM_W: f32 = 26.0;
const AXIS_W: f32 = 64.0;
const RANGE_W: f32 = 84.0;
/// What the table needs across: its cells, the gaps between them and the row's side padding.
const TABLE_W: f32 = NUM_W + 3.0 * AXIS_W + RANGE_W + 6.0 * spai_fit_ui::rows::GAP;

/// `own_scroll`: the probe rows scroll by themselves, for a table with a fixed height to fill.
fn table(ui: &mut egui::Ui, st: &mut State, f: &mut Formation, mode: Mode, own_scroll: bool) {
    let unit = mode.unit();
    ui.horizontal(|ui| {
        ui.label(tr!("Name"));
        ui.add_sized([ui.available_width(), 22.0], egui::TextEdit::singleline(&mut f.name));
    });
    ui.add_space(4.0);
    let (decimals, speed) = match mode {
        Mode::Scan => (2, 0.05),
        Mode::Perch => (0, 50.0),
    };
    let head = match mode {
        Mode::Scan => tr!("Offsets from the centre in AU. Y is up."),
        Mode::Perch => tr!("Offsets from the centre in km. Y is up."),
    };
    clipped(ui, egui::RichText::new(head).weak(), head);
    spai_fit_ui::rows::row(ui, "probe_head", "head", ROW_H - 4.0, false, |ui| {
        spai_fit_ui::rows::head_cell(ui, NUM_W, "#", false);
        for h in ["X", "Y", "Z"] {
            spai_fit_ui::rows::head_cell(ui, AXIS_W, h, false);
        }
        spai_fit_ui::rows::head_cell(ui, RANGE_W, tr!("Range"), false);
    });
    let mut picked = None;
    let room = if own_scroll { (ui.available_height() - 76.0).max(ROW_H * 3.0) } else { f32::INFINITY };
    let out = egui::ScrollArea::vertical().id_salt("probe_rows").max_height(room).auto_shrink([false, true]).show(ui, |ui| {
        for (i, p) in f.probes.iter_mut().enumerate() {
            let resp = spai_fit_ui::rows::row(ui, "probe_rows", i, ROW_H - 4.0, st.probe == Some(i), |ui| {
                spai_fit_ui::rows::cell(ui, NUM_W, egui::RichText::new((i + 1).to_string()), "");
                for axis in 0..3 {
                    let mut v = p.pos[axis] / unit;
                    let drag = egui::DragValue::new(&mut v).speed(speed).fixed_decimals(decimals);
                    if ui.add_sized([AXIS_W, 22.0], drag).changed() {
                        p.pos[axis] = v * unit;
                        picked = Some(i);
                    }
                }
                egui::ComboBox::from_id_salt(("probe_range", i)).width(RANGE_W - 8.0).selected_text(range_text(p.range)).show_ui(ui, |ui| {
                    for au in RANGES_AU {
                        if ui.menu_label((p.range - au * AU).abs() < 1.0, range_text(au * AU)).clicked() {
                            p.range = au * AU;
                        }
                    }
                });
            });
            if resp.clicked() {
                picked = Some(i);
            }
        }
    });
    spai_fit_ui::rows::note_scrolled(ui.ctx(), &out);
    if picked.is_some() {
        st.probe = picked;
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let full = f.probes.len() >= MAX_PROBES;
        let add = ui.add_enabled(!full, egui::Button::new(trf!("{icon}  Probe", icon = icon::PLUS)));
        if add.on_hover_text(tr!("Add a probe at the centre")).on_disabled_hover_text(trf!("A launcher holds {n} probes.", n = MAX_PROBES)).clicked() {
            let range = f.probes.last().map_or(4.0 * AU, |p| p.range);
            f.probes.push(Probe { pos: [0.0; 3], range });
            st.probe = Some(f.probes.len() - 1);
        }
        let del = ui.add_enabled(st.probe.is_some(), egui::Button::new(trf!("{icon}  Remove", icon = icon::MINUS)));
        if del.on_hover_text(tr!("Remove the selected probe")).on_disabled_hover_text(tr!("Select a probe first.")).clicked() {
            if let Some(i) = st.probe.take().filter(|i| *i < f.probes.len()) {
                f.probes.remove(i);
            }
        }
        let far = distance_text(f.extent(), mode);
        let text = trf!("Probes: {n}, farthest {far}", n = f.probes.len(), far = far);
        clipped(ui, egui::RichText::new(&text).weak(), &text);
    });
    ui.add_space(4.0);
    if st.radius <= 0.0 {
        st.radius = match mode {
            Mode::Scan => 2.0,
            Mode::Perch => 5_000.0,
        };
    }
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("probe_shape").width(84.0).selected_text(spai_ui::i18n::t(st.shape.label())).show_ui(ui, |ui| {
            for shape in Shape::ALL {
                ui.menu_value(&mut st.shape, shape, spai_ui::i18n::t(shape.label()));
            }
        });
        ui.add(egui::DragValue::new(&mut st.count).range(1..=MAX_PROBES)).on_hover_text(tr!("Probes"));
        let suffix = if mode == Mode::Scan { " AU" } else { " km" };
        ui.add(egui::DragValue::new(&mut st.radius).speed(speed).range(0.0..=f64::MAX).fixed_decimals(decimals).suffix(suffix))
            .on_hover_text(tr!("How far out the shape reaches"));
        if ui.button(tr!("Arrange")).on_hover_text(tr!("Replace the probes with this shape, keeping the first probe's range")).clicked() {
            let range = f.probes.first().map_or(4.0 * AU, |p| p.range);
            f.probes = probes::generate(st.shape, st.count, st.radius * unit, range);
            st.probe = None;
        }
    });
}

fn scene(ui: &mut egui::Ui, st: &mut State, f: &mut Formation, mode: Mode, rect: egui::Rect) {
    let resp = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, tr!("Formation view")));
    let radius = view_radius(f, mode);
    let cam = Camera {
        yaw: st.yaw,
        pitch: st.pitch,
        scale: f64::from(rect.width().min(rect.height())) * 0.45 / radius * f64::from(st.zoom),
        center: rect.center(),
    };
    let editable = ui.is_enabled();

    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            st.zoom = (st.zoom * (scroll * 0.003).exp()).clamp(0.15, 30.0);
        }
    }
    let pointer = resp.interact_pointer_pos().or_else(|| resp.hover_pos());
    // A drag registers after the pointer has travelled: what it grabs is what was under the press.
    let pressed = ui.input(|i| i.pointer.press_origin()).filter(|_| resp.is_pointer_button_down_on() || resp.dragged());
    let over = pressed.or(pointer).and_then(|p| pick(&cam, &f.probes, p, PICK_PX));
    if resp.drag_started() {
        st.dragging = over.filter(|_| editable);
        if over.is_some() {
            st.probe = over;
        }
    }
    if resp.clicked() {
        st.probe = over;
    }
    if resp.dragged() {
        match st.dragging.and_then(|i| f.probes.get_mut(i)) {
            Some(p) => {
                let free = !st.snap || ui.input(|i| i.modifiers.alt);
                let step = mode.snap();
                if ui.input(|i| i.modifiers.shift) {
                    p.pos[1] += cam.lift(resp.drag_delta().y);
                } else if let Some(to) = pointer.and_then(|at| cam.on_plane(at, p.pos[1])) {
                    let limit = radius * 20.0;
                    p.pos[0] = (if free { to[0] } else { snapped(to[0], step) }).clamp(-limit, limit);
                    p.pos[2] = (if free { to[2] } else { snapped(to[2], step) }).clamp(-limit, limit);
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            None => {
                let d = resp.drag_delta();
                st.yaw += d.x * 0.008;
                st.pitch = (st.pitch + d.y * 0.008).clamp(0.08, 1.5);
            }
        }
    } else {
        st.dragging = None;
        if over.is_some() && editable {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    if !ui.is_rect_visible(rect) {
        return;
    }
    let p = ui.painter_at(rect);
    let v = ui.visuals();
    p.rect_filled(rect, 2.0, v.extreme_bg_color);
    let grid = v.widgets.noninteractive.bg_stroke.color;
    let weak = v.weak_text_color();
    let accent = v.selection.stroke.color;
    let font = egui::TextStyle::Body.resolve(ui.style());

    let ring = |r: f64, y: f64, stroke: egui::Stroke, at: [f64; 3]| {
        let pts: Vec<egui::Pos2> = (0..=72)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 72.0;
                cam.project([at[0] + r * a.cos(), at[1] + y, at[2] + r * a.sin()]).0
            })
            .collect();
        p.add(egui::Shape::line(pts, stroke));
    };
    // Stepped in the mode's unit, so the rings read 2 AU or 5,000 km and not a fraction.
    let step = ring_step(radius / mode.unit()) * mode.unit();
    let mut r = step;
    while r <= radius * 1.01 {
        ring(r, 0.0, egui::Stroke::new(1.0, grid), [0.0; 3]);
        let (at, _) = cam.project([r, 0.0, 0.0]);
        p.text(at + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, distance_text(r, mode), font.clone(), weak);
        r += step;
    }
    if mode == Mode::Perch && radius >= GRID_KM * KM * 0.6 {
        ring(GRID_KM * KM, 0.0, egui::Stroke::new(1.5, standing::WARNING.gamma_multiply(0.7)), [0.0; 3]);
    }
    for axis in [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]] {
        let a = cam.project([axis[0] * radius, 0.0, axis[2] * radius]).0;
        let b = cam.project([-axis[0] * radius, 0.0, -axis[2] * radius]).0;
        p.line_segment([a, b], egui::Stroke::new(1.0, grid));
    }
    let (origin, _) = cam.project([0.0; 3]);
    p.circle_filled(origin, 3.0, weak);

    let mut order: Vec<(usize, egui::Pos2, f64)> =
        f.probes.iter().enumerate().map(|(i, pr)| (i, cam.project(pr.pos).0, cam.project(pr.pos).1)).collect();
    order.sort_by(|a, b| a.2.total_cmp(&b.2));
    for (i, at, _) in &order {
        let probe = &f.probes[*i];
        let selected = st.probe == Some(*i);
        let color = if selected { accent } else { crate::theme::chip::PROBES };
        let (foot, _) = cam.project([probe.pos[0], 0.0, probe.pos[2]]);
        p.line_segment([foot, *at], egui::Stroke::new(1.0, color.gamma_multiply(0.5)));
        p.circle_filled(foot, 2.0, color.gamma_multiply(0.6));
        if mode == Mode::Scan {
            let px = (probe.range * cam.scale) as f32;
            p.circle_filled(*at, px, color.gamma_multiply(if selected { 0.10 } else { 0.05 }));
            p.circle_stroke(*at, px, egui::Stroke::new(1.0, color.gamma_multiply(if selected { 0.9 } else { 0.4 })));
            ring(probe.range, 0.0, egui::Stroke::new(1.0, color.gamma_multiply(0.3)), probe.pos);
        }
    }
    for (i, at, _) in &order {
        let selected = st.probe == Some(*i);
        let color = if selected { accent } else { crate::theme::chip::PROBES };
        p.circle_filled(*at, if selected { 6.0 } else { 4.5 }, color);
        if selected {
            p.circle_stroke(*at, 9.0, egui::Stroke::new(1.5, color));
        }
        let label = if selected {
            format!("{}  {}", i + 1, distance_text(probes::length(f.probes[*i].pos), mode))
        } else {
            (i + 1).to_string()
        };
        p.text(*at + egui::vec2(9.0, -9.0), egui::Align2::LEFT_BOTTOM, label, font.clone(), v.text_color());
    }
    let hint = tr!("Drag a probe to move it, with Shift to raise it. Drag the view to turn it, scroll to zoom.");
    let galley = p.layout(hint.to_owned(), font, weak, rect.width() - 16.0);
    p.galley(egui::pos2(rect.left() + 8.0, rect.bottom() - galley.size().y - 6.0), galley, weak);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> Camera {
        Camera { yaw: 0.6, pitch: 0.5, scale: 0.001, center: egui::pos2(400.0, 300.0) }
    }

    #[test]
    fn a_point_on_the_plane_projects_and_comes_back() {
        let c = cam();
        for p in [[1000.0, 0.0, -2500.0], [-40_000.0, 5000.0, 12_000.0], [0.0, -300.0, 0.0]] {
            let (at, _) = c.project(p);
            let back = c.on_plane(at, p[1]).unwrap();
            for axis in 0..3 {
                assert!((back[axis] - p[axis]).abs() < 1.0, "{p:?} came back as {back:?}");
            }
        }
        assert!(Camera { pitch: 0.0, ..c }.on_plane(egui::pos2(1.0, 1.0), 0.0).is_none());
    }

    #[test]
    fn up_is_up_and_nearer_is_deeper() {
        let c = cam();
        assert!(c.project([0.0, 1000.0, 0.0]).0.y < c.project([0.0, 0.0, 0.0]).0.y);
        assert_eq!(c.project([0.0; 3]).0, egui::pos2(400.0, 300.0));
        assert!(c.project([0.0, 1000.0, 0.0]).1 > c.project([0.0; 3]).1);
        let lift = c.lift(-10.0);
        assert!(lift > 0.0);
        let (a, _) = c.project([0.0, lift, 0.0]);
        assert!((a.y - 290.0).abs() < 0.01);
    }

    #[test]
    fn pick_takes_the_probe_under_the_pointer() {
        let c = cam();
        let probes = [
            Probe { pos: [0.0; 3], range: AU },
            Probe { pos: [100_000.0, 0.0, 0.0], range: AU },
        ];
        let (at, _) = c.project(probes[1].pos);
        assert_eq!(pick(&c, &probes, at + egui::vec2(3.0, -2.0), PICK_PX), Some(1));
        assert_eq!(pick(&c, &probes, at + egui::vec2(40.0, 0.0), PICK_PX), None);
    }

    #[test]
    fn rings_land_on_round_distances() {
        assert_eq!(ring_step(30_000.0), 10_000.0);
        assert_eq!(ring_step(8.0), 2.0);
        assert_eq!(ring_step(1_000.0), 200.0);
        assert_eq!(ring_step(2.4), 0.5);
    }

    #[test]
    fn distances_read_in_the_mode_unit() {
        assert_eq!(distance_text(12_400.0 * KM, Mode::Perch), "12,400 km");
        assert_eq!(distance_text(150.0 * KM, Mode::Perch), "150 km");
        assert_eq!(distance_text(1_234_567.0 * KM, Mode::Perch), "1,234,567 km");
        assert_eq!(distance_text(4.0 * AU, Mode::Scan), "4 AU");
        assert_eq!(distance_text(0.25 * AU, Mode::Scan), "0.25 AU");
        assert_eq!(distance_text(0.0, Mode::Scan), "0 AU");
    }

    #[test]
    fn the_mode_is_guessed_from_how_far_out_the_probes_are() {
        let f = |radius: f64| Formation { id: 0, name: String::new(), probes: probes::generate(Shape::Ring, 8, radius, AU) };
        assert_eq!(Mode::guess(&f(5_000.0 * KM)), Mode::Perch);
        assert_eq!(Mode::guess(&f(30_000.0 * KM)), Mode::Perch);
        assert_eq!(Mode::guess(&f(4.0 * AU)), Mode::Scan);
        assert_eq!(Mode::guess(&Formation { id: 0, name: String::new(), probes: vec![] }), Mode::Scan);
    }

    #[test]
    fn new_formations_fit_their_mode() {
        let all = probes::Formations::default();
        let scan = new_formation(&all, Mode::Scan);
        assert_eq!((scan.probes.len(), Mode::guess(&scan)), (7, Mode::Scan));
        let perch = new_formation(&all, Mode::Perch);
        assert_eq!((perch.probes.len(), Mode::guess(&perch)), (8, Mode::Perch));
        assert!(perch.extent() <= GRID_KM * KM);
    }
}
