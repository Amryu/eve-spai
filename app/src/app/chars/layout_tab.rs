//! The Windows tab: the client's screen drawn to scale, with every window movable and resizable.

use super::*;
use crate::evesettings::layout::{Filter, Layout, Mode, Shown, MIN_SIZE};

const COMMON_SCREENS: [[i32; 2]; 4] = [[1920, 1080], [2560, 1440], [3440, 1440], [3840, 2160]];
/// Screen pixels within which an edge is grabbed, or snaps to another edge.
const GRAB: i32 = 10;
const SNAP: i32 = 12;
/// The ship HUD's footprint and the locked-target row's, in screen pixels.
const HUD: [i32; 2] = [420, 150];
const TARGET: [i32; 2] = [94, 110];
const TARGETS_SHOWN: i32 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edges {
    pub(crate) left: bool,
    pub(crate) right: bool,
    pub(crate) top: bool,
    pub(crate) bottom: bool,
}

impl Edges {
    fn any(self) -> bool {
        self.left || self.right || self.top || self.bottom
    }

    fn cursor(self) -> egui::CursorIcon {
        match (self.left || self.right, self.top || self.bottom) {
            (true, true) if self.left == self.top => egui::CursorIcon::ResizeNwSe,
            (true, true) => egui::CursorIcon::ResizeNeSw,
            (true, false) => egui::CursorIcon::ResizeHorizontal,
            (false, true) => egui::CursorIcon::ResizeVertical,
            (false, false) => egui::CursorIcon::Grab,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Grabbed {
    Window { id: String, edges: Edges, rect: [i32; 4] },
    Hud { offset: i32 },
    Targets { at: [f64; 2] },
}

#[derive(Clone, Debug)]
struct Drag {
    what: Grabbed,
    from: [f32; 2],
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) filter: Filter,
    screen: Option<[i32; 2]>,
    pub(crate) selected: Option<String>,
    drag: Option<Drag>,
}

fn contains(r: [i32; 4], p: [i32; 2], pad: i32) -> bool {
    p[0] >= r[0] - pad && p[0] <= r[0] + r[2] + pad && p[1] >= r[1] - pad && p[1] <= r[1] + r[3] + pad
}

/// The window under `p` and which of its edges are within reach. Of overlapping windows the
/// smallest wins, since it is the one that would otherwise be impossible to reach.
pub(crate) fn grab_at(shown: &[Shown], p: [i32; 2], reach: i32) -> Option<(usize, Edges)> {
    let (i, s) = shown
        .iter()
        .enumerate()
        .filter(|(_, s)| contains(s.rect, p, reach / 2))
        .min_by_key(|(_, s)| i64::from(s.rect[2]) * i64::from(s.rect[3]))?;
    let [x, y, w, h] = s.rect;
    // On a small window the corners would otherwise swallow the whole body.
    let reach = reach.min(w / 4).min(h / 4);
    let edges = Edges {
        left: p[0] - x < reach,
        right: x + w - p[0] < reach,
        top: p[1] - y < reach,
        bottom: y + h - p[1] < reach,
    };
    Some((i, if s.locked { Edges::default() } else { edges }))
}

/// `start` moved by the pointer delta, or with its grabbed edges moved, kept on the screen and
/// above the minimum size.
pub(crate) fn drag_rect(start: [i32; 4], edges: Edges, d: [i32; 2], screen: [i32; 2]) -> [i32; 4] {
    let [x, y, w, h] = start;
    if !edges.any() {
        return [(x + d[0]).clamp(0, (screen[0] - w).max(0)), (y + d[1]).clamp(0, (screen[1] - h).max(0)), w, h];
    }
    let (mut l, mut t, mut r, mut b) = (x, y, x + w, y + h);
    if edges.left {
        l = (l + d[0]).clamp(0, r - MIN_SIZE);
    }
    if edges.right {
        r = (r + d[0]).clamp(l + MIN_SIZE, screen[0].max(l + MIN_SIZE));
    }
    if edges.top {
        t = (t + d[1]).clamp(0, b - MIN_SIZE);
    }
    if edges.bottom {
        b = (b + d[1]).clamp(t + MIN_SIZE, screen[1].max(t + MIN_SIZE));
    }
    [l, t, r - l, b - t]
}

fn nearest(value: i32, lines: &[i32], reach: i32) -> Option<i32> {
    lines.iter().copied().filter(|l| (l - value).abs() <= reach).min_by_key(|l| (l - value).abs())
}

/// Pulls the edges being dragged onto the screen edges and the edges of the other windows.
pub(crate) fn snap(rect: [i32; 4], edges: Edges, others: &[[i32; 4]], screen: [i32; 2], reach: i32) -> [i32; 4] {
    let mut xs = vec![0, screen[0]];
    let mut ys = vec![0, screen[1]];
    for o in others {
        xs.extend([o[0], o[0] + o[2]]);
        ys.extend([o[1], o[1] + o[3]]);
    }
    let [mut x, mut y, mut w, mut h] = rect;
    if edges.any() {
        if edges.left {
            if let Some(l) = nearest(x, &xs, reach).filter(|l| x + w - l >= MIN_SIZE) {
                w += x - l;
                x = l;
            }
        }
        if edges.right {
            if let Some(r) = nearest(x + w, &xs, reach).filter(|r| r - x >= MIN_SIZE) {
                w = r - x;
            }
        }
        if edges.top {
            if let Some(t) = nearest(y, &ys, reach).filter(|t| y + h - t >= MIN_SIZE) {
                h += y - t;
                y = t;
            }
        }
        if edges.bottom {
            if let Some(b) = nearest(y + h, &ys, reach).filter(|b| b - y >= MIN_SIZE) {
                h = b - y;
            }
        }
        return [x, y, w, h];
    }
    // A moved window keeps its size: whichever of its two edges is closer to a line wins.
    let pull = |pos: i32, size: i32, lines: &[i32]| {
        let lead = nearest(pos, lines, reach).map(|l| l - pos);
        let trail = nearest(pos + size, lines, reach).map(|l| l - pos - size);
        match (lead, trail) {
            (Some(a), Some(b)) => pos + if a.abs() <= b.abs() { a } else { b },
            (Some(a), None) | (None, Some(a)) => pos + a,
            (None, None) => pos,
        }
    };
    x = pull(x, w, &xs).clamp(0, (screen[0] - w).max(0));
    y = pull(y, h, &ys).clamp(0, (screen[1] - h).max(0));
    [x, y, w, h]
}

fn hud_rect(offset: i32, screen: [i32; 2]) -> [i32; 4] {
    [screen[0] / 2 + offset - HUD[0] / 2, screen[1] - HUD[1], HUD[0], HUD[1]]
}

/// The targets hang leftwards from their anchor, newest furthest out.
fn targets_rect(at: [f64; 2], screen: [i32; 2]) -> [i32; 4] {
    let (x, y) = ((at[0] * f64::from(screen[0])) as i32, (at[1] * f64::from(screen[1])) as i32);
    [x - TARGET[0] * (TARGETS_SHOWN - 1), y, TARGET[0] * TARGETS_SHOWN, TARGET[1]]
}

fn glyph_for(id: &str) -> &'static str {
    match id {
        _ if id.starts_with("overview") => icon::LIST_DASHES,
        "selecteditemview" => icon::CROSSHAIR_SIMPLE,
        "droneview" => icon::DRONE,
        "probeScannerWindow" => icon::BROADCAST,
        "directionalScannerWindow" => icon::WIFI_HIGH,
        "standaloneBookmarkWnd" | "locations" => icon::MAP_PIN,
        "fleetwindow" => icon::USERS_THREE,
        "watchlistpanel" => icon::EYE,
        "lobbyWnd" => icon::BUILDINGS,
        "market" => icon::CHART_LINE_UP,
        "walletWindow" => icon::WALLET,
        "mail" => icon::ENVELOPE_SIMPLE,
        "notepad" => icon::NOTEPAD,
        "addressbook" => icon::ADDRESS_BOOK,
        "charactersheet" => icon::USER_CIRCLE,
        "fittingWnd" => icon::ROCKET,
        _ if id.starts_with("Inventory") || id == "assets" => icon::PACKAGE,
        _ if id.starts_with("chatchannel_") => icon::CHAT_CIRCLE,
        _ => icon::APP_WINDOW,
    }
}

fn mode_label(m: Mode) -> &'static str {
    match m {
        Mode::Space => tr!("In flight"),
        Mode::Station => tr!("In a station"),
        Mode::Structure => tr!("In a structure"),
    }
}

fn title(l: &Layout, s: &Shown) -> String {
    s.members.iter().map(|m| l.label(m)).collect::<Vec<_>>().join(", ")
}

pub(crate) fn ui(ui: &mut egui::Ui, st: &mut State, s: &mut Session) {
    let screen = *st.screen.get_or_insert_with(|| s.layout.now.resolution());
    let filter = st.filter;
    let shown = s.layout.now.shown(filter, screen);
    if st.selected.as_ref().is_some_and(|id| !shown.iter().any(|w| w.id == *id)) {
        st.selected = None;
    }

    ui.add_space(4.0);
    // The two dropdowns lead the row: a nested ui at the end of a wrapping row is handed what
    // is left of the line and breaks its label over many lines.
    ui.horizontal_wrapped(|ui| {
        let addable = s.layout.now.addable(filter);
        ui.add_enabled_ui(!addable.is_empty() && !s.read_only, |ui| {
            ui.menu_button(trf!("{icon}  Open a window", icon = icon::PLUS), |ui| {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for id in &addable {
                        let label = format!("{}  {}", glyph_for(id), s.layout.now.label(id));
                        if ui.button(label).clicked() {
                            s.layout.now.add(id, screen);
                            st.selected = Some(id.clone());
                            ui.close();
                        }
                    }
                });
            });
        });
        let mut screens: Vec<[i32; 2]> = s.layout.now.resolutions();
        screens.extend(COMMON_SCREENS);
        screens.push(screen);
        screens.sort_unstable();
        screens.dedup();
        egui::ComboBox::from_id_salt("layout_screen")
            .selected_text(format!("{} × {}", screen[0], screen[1]))
            .show_ui(ui, |ui| {
                for sc in screens {
                    if ui.menu_label(sc == screen, format!("{} × {}", sc[0], sc[1])).clicked() {
                        st.screen = Some(sc);
                    }
                }
            })
            .response
            .on_hover_text(tr!("The game's resolution. Windows last placed on another one are shown where the game would put them."));
        ui.separator();
        for m in [Mode::Space, Mode::Station, Mode::Structure] {
            if ui.menu_label(st.filter.mode == m, mode_label(m)).clicked() {
                st.filter.mode = m;
            }
        }
        ui.separator();
        ui.checkbox(&mut st.filter.fleet, tr!("In a fleet")).on_hover_text(tr!("Show the windows that only exist while in a fleet"));
        ui.checkbox(&mut st.filter.drones, tr!("With drones")).on_hover_text(tr!("Show the drone window, which needs a ship that carries drones"));
        ui.checkbox(&mut st.filter.minimized, tr!("Minimized ones")).on_hover_text(tr!("Show minimized windows where they would come back to"));
    });

    selection_bar(ui, st, s, &shown, screen);

    let avail = ui.available_rect_before_wrap();
    let scale = (avail.width() / screen[0] as f32).min(avail.height() / screen[1] as f32).max(0.01);
    let size = egui::vec2(screen[0] as f32 * scale, screen[1] as f32 * scale);
    let canvas = egui::Rect::from_min_size(egui::pos2(avail.center().x - size.x / 2.0, avail.top()), size);
    let resp = ui.allocate_rect(canvas, egui::Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, tr!("Window layout")));
    let to_ui = |r: [i32; 4]| {
        egui::Rect::from_min_size(
            canvas.min + egui::vec2(r[0] as f32, r[1] as f32) * scale,
            egui::vec2(r[2] as f32, r[3] as f32) * scale,
        )
    };
    let to_screen = |p: egui::Pos2| [((p.x - canvas.left()) / scale), ((p.y - canvas.top()) / scale)];

    let hud = hud_rect(s.layout.now.hud_offset.unwrap_or(0), screen);
    let targets = s.targets.now.map(|at| targets_rect(at, screen));
    let editable = ui.is_enabled() && !s.read_only;

    // What the pointer is over, for the cursor and for starting a drag. A drag only registers
    // once the pointer has travelled, so what it grabs is whatever was under the press.
    let pressed = ui.input(|i| i.pointer.press_origin()).filter(|_| resp.is_pointer_button_down_on() || resp.dragged());
    let hover = pressed.or(resp.hover_pos()).map(to_screen).map(|p| [p[0] as i32, p[1] as i32]);
    let over = hover.and_then(|p| {
        if let Some((i, edges)) = grab_at(&shown, p, GRAB) {
            return Some(Grabbed::Window { id: shown[i].id.clone(), edges, rect: shown[i].rect });
        }
        if targets.is_some_and(|t| contains(t, p, 0)) {
            return s.targets.now.map(|at| Grabbed::Targets { at });
        }
        contains(hud, p, 0).then(|| Grabbed::Hud { offset: s.layout.now.hud_offset.unwrap_or(0) })
    });
    if editable && st.drag.is_none() {
        match &over {
            Some(Grabbed::Window { edges, .. }) => ui.ctx().set_cursor_icon(edges.cursor()),
            Some(_) => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
            None => {}
        }
    }
    if resp.clicked() || resp.secondary_clicked() || resp.drag_started() {
        st.selected = match &over {
            Some(Grabbed::Window { id, .. }) => Some(id.clone()),
            _ => None,
        };
    }
    if editable && resp.drag_started() {
        if let (Some(what), Some(p)) = (over.clone(), pressed.or(resp.interact_pointer_pos())) {
            let locked = matches!(&what, Grabbed::Window { id, .. } if shown.iter().any(|w| w.id == *id && w.locked));
            if !locked {
                st.drag = Some(Drag { what, from: to_screen(p) });
            }
        }
    }
    if let (Some(drag), Some(p)) = (&st.drag, resp.interact_pointer_pos()) {
        let now = to_screen(p);
        let d = [(now[0] - drag.from[0]).round() as i32, (now[1] - drag.from[1]).round() as i32];
        match &drag.what {
            Grabbed::Window { id, edges, rect } => {
                let others: Vec<[i32; 4]> = shown.iter().filter(|w| w.id != *id).map(|w| w.rect).collect();
                let moved = drag_rect(*rect, *edges, d, screen);
                // Alt drags freely, as it does in the game.
                let placed = if ui.input(|i| i.modifiers.alt) { moved } else { snap(moved, *edges, &others, screen, SNAP) };
                s.layout.now.set_rect(id, placed, screen);
                ui.ctx().set_cursor_icon(if edges.any() { edges.cursor() } else { egui::CursorIcon::Grabbing });
            }
            Grabbed::Hud { offset } => {
                let reach = (screen[0] - HUD[0]) / 2;
                let mut to = (offset + d[0]).clamp(-reach, reach);
                if to.abs() <= SNAP {
                    to = 0;
                }
                s.layout.now.hud_offset = Some(to);
            }
            Grabbed::Targets { at } => {
                let x = (at[0] + f64::from(d[0]) / f64::from(screen[0])).clamp(0.05, 1.0);
                let y = (at[1] + f64::from(d[1]) / f64::from(screen[1])).clamp(0.0, 0.9);
                s.targets.now = Some([x, y]);
            }
        }
    }
    if !resp.dragged() {
        st.drag = None;
    }
    if editable {
        resp.context_menu(|ui| match st.selected.clone() {
            Some(id) => window_actions(ui, st, s, &shown, &id, screen, true),
            None => {
                ui.label(egui::RichText::new(tr!("Right-click a window for its actions.")).weak());
            }
        });
    }

    if !ui.is_rect_visible(canvas) {
        return;
    }
    let p = ui.painter_at(canvas);
    let v = ui.visuals();
    p.rect_filled(canvas, 2.0, v.extreme_bg_color);
    let line = v.widgets.noninteractive.bg_stroke.color;
    let weak = v.weak_text_color();

    let bar = to_ui([0, 0, s.layout.now.neocom_width.clamp(24, 80), screen[1]]);
    p.rect_filled(bar, 0.0, v.faint_bg_color);
    let buttons = s.neocom.now.as_ref().map_or(12, |b| b.len()) as i32;
    let side = bar.width() * 0.7;
    for i in 0..buttons {
        let c = egui::pos2(bar.center().x, bar.top() + bar.width() * (0.6 + i as f32));
        if c.y + side > bar.bottom() {
            break;
        }
        p.rect_filled(egui::Rect::from_center_size(c, egui::Vec2::splat(side)), 1.0, line);
    }

    let hud_ui = to_ui(hud);
    let r = hud_ui.height() * 0.42;
    let c = egui::pos2(hud_ui.center().x, hud_ui.bottom() - r - 2.0);
    p.circle_stroke(c, r, egui::Stroke::new(1.5, weak));
    p.circle_stroke(c, r * 0.55, egui::Stroke::new(1.0, line));
    for i in 0..8 {
        let slot = egui::pos2(c.x + r * 1.5 + (i % 4) as f32 * r * 0.62, c.y - r * 0.35 + (i / 4) as f32 * r * 0.7);
        p.circle_stroke(slot, r * 0.26, egui::Stroke::new(1.0, line));
    }
    let body = egui::TextStyle::Body.resolve(ui.style());
    if let Some(t) = targets {
        let t = to_ui(t);
        let step = t.width() / TARGETS_SHOWN as f32;
        for i in 0..TARGETS_SHOWN {
            let c = egui::pos2(t.right() - step * (i as f32 + 0.5), t.top() + step * 0.5);
            p.circle_stroke(c, step * 0.36, egui::Stroke::new(1.5, weak));
            // Numbered only where a body-size digit fits the ring; never shrunk to fit.
            if step * 0.72 >= body.size + 6.0 {
                p.text(c, egui::Align2::CENTER_CENTER, (i + 1).to_string(), body.clone(), weak);
            }
        }
    }

    let font = egui::TextStyle::Body.resolve(ui.style());
    let draw = |w: &Shown| {
        let r = to_ui(w.rect).shrink(0.5);
        let selected = st.selected.as_deref() == Some(w.id.as_str());
        // A minimized window is only an outline of where it comes back to.
        let fill = if selected {
            v.selection.bg_fill.gamma_multiply(0.45)
        } else if w.minimized {
            v.panel_fill.gamma_multiply(0.25)
        } else {
            v.panel_fill.gamma_multiply(0.92)
        };
        p.rect_filled(r, 2.0, fill);
        let stroke = if selected { egui::Stroke::new(1.5, v.selection.stroke.color) } else { egui::Stroke::new(1.0, line) };
        p.rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
        let text = if w.minimized { weak } else { v.text_color() };
        let clip = p.with_clip_rect(r.shrink(2.0).intersect(canvas));
        let row = font.size + 3.0;
        let fit = (((r.height() - 8.0) / row).floor() as usize).max(1);
        let mut lines: Vec<String> = w.members.iter().map(|m| s.layout.now.label(m)).collect();
        if w.minimized {
            lines.push(tr!("(minimized)").to_owned());
        }
        let take = lines.len().min(fit);
        let top = r.center().y - row * take as f32 / 2.0;
        // The corner marks only where they clear the label.
        if top - r.top() >= 22.0 && r.width() >= 48.0 {
            let glyph = w.members.first().map_or(icon::APP_WINDOW, |m| glyph_for(m));
            clip.text(r.min + egui::vec2(5.0, 4.0), egui::Align2::LEFT_TOP, glyph, egui::FontId::proportional(16.0), weak);
            if w.locked {
                clip.text(r.right_top() + egui::vec2(-5.0, 4.0), egui::Align2::RIGHT_TOP, icon::LOCK, egui::FontId::proportional(13.0), weak);
            }
        }
        for (i, label) in lines.iter().take(take).enumerate() {
            let last_of_cut = i + 1 == take && take < lines.len();
            let label = if last_of_cut { format!("+{}", lines.len() - i) } else { label.clone() };
            let galley = egui::WidgetText::from(label).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                (r.width() - 10.0).max(8.0),
                egui::TextStyle::Body,
            );
            let pos = egui::pos2(r.center().x - galley.size().x / 2.0, top + row * i as f32);
            clip.galley(pos, galley, text);
        }
    };
    let selected = |w: &&Shown| st.selected.as_deref() == Some(w.id.as_str());
    for w in shown.iter().filter(|w| w.minimized && !selected(w)) {
        draw(w);
    }
    for w in shown.iter().filter(|w| !w.minimized && !selected(w)) {
        draw(w);
    }
    for w in shown.iter().filter(|w| st.selected.as_deref() == Some(w.id.as_str())) {
        draw(w);
    }
    if shown.is_empty() {
        p.text(canvas.center(), egui::Align2::CENTER_CENTER, tr!("No windows stored for this view. Open one above."), font, weak);
    }
}

/// One row that is always there: the selected window and what can be done with it, or a hint.
fn selection_bar(ui: &mut egui::Ui, st: &mut State, s: &mut Session, shown: &[Shown], screen: [i32; 2]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let ui = &mut ui;
    let Some(w) = st.selected.as_ref().and_then(|id| shown.iter().find(|w| w.id == *id)).cloned() else {
        let hint = tr!("Drag a window to move it, an edge to resize it. The ship HUD and the locked targets move too. Right-click for more.");
        clipped(ui, egui::RichText::new(hint).weak(), hint);
        return;
    };
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if !s.read_only {
            window_actions(ui, st, s, shown, &w.id, screen, false);
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let name = title(&s.layout.now, &w);
            let text = format!("{name}  ·  {} × {} @ {}, {}", w.rect[2], w.rect[3], w.rect[0], w.rect[1]);
            clipped(ui, egui::RichText::new(&text).strong(), &text);
        });
    });
}

/// The actions for one window, as a menu (`menu`) or as a row of icon buttons.
fn window_actions(ui: &mut egui::Ui, st: &mut State, s: &mut Session, shown: &[Shown], id: &str, screen: [i32; 2], menu: bool) {
    let Some(w) = shown.iter().find(|w| w.id == id).cloned() else {
        return;
    };
    let act = |ui: &mut egui::Ui, glyph: &str, text: &str| -> bool {
        if menu {
            ui.button(format!("{glyph}  {text}")).clicked()
        } else {
            spai_ui::widgets::icon_button(ui, glyph).on_hover_text(text).clicked()
        }
    };
    let l = &mut s.layout.now;
    let mut done = false;
    if act(ui, icon::X, tr!("Close")) {
        l.close(id);
        st.selected = None;
        done = true;
    }
    if w.minimized {
        if act(ui, icon::CORNERS_OUT, tr!("Restore")) {
            l.set_minimized(id, false);
            done = true;
        }
    } else if act(ui, icon::MINUS, tr!("Minimize")) {
        l.set_minimized(id, true);
        done = true;
    }
    if w.locked {
        if act(ui, icon::LOCK_OPEN, tr!("Unlock")) {
            l.set_locked(id, false);
            done = true;
        }
    } else if act(ui, icon::LOCK, tr!("Lock in place")) {
        l.set_locked(id, true);
        done = true;
    }
    let others: Vec<&Shown> = shown.iter().filter(|o| o.id != id).collect();
    if !others.is_empty() {
        let label = if menu { trf!("{icon}  Stack with", icon = icon::STACK) } else { icon::STACK.to_owned() };
        let r = ui.menu_button(label, |ui| {
            egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                for o in &others {
                    if ui.button(title(l, o)).clicked() {
                        l.stack_onto(id, &o.id);
                        st.selected = l.wins.get(&o.id).and_then(|w| w.stack.clone()).or(Some(o.id.clone()));
                        ui.close();
                    }
                }
            });
        });
        if !menu {
            r.response.on_hover_text(tr!("Stack with another window, as tabs"));
        }
    }
    if w.stacked {
        let label = if menu { trf!("{icon}  Tabs", icon = icon::STACK_MINUS) } else { icon::STACK_MINUS.to_owned() };
        let r = ui.menu_button(label, |ui| {
            for m in &w.members {
                ui.horizontal(|ui| {
                    if spai_ui::widgets::icon_button(ui, icon::ARROW_UP).on_hover_text(tr!("Earlier tab")).clicked() {
                        l.move_in_stack(id, m, -1);
                    }
                    if spai_ui::widgets::icon_button(ui, icon::ARROW_DOWN).on_hover_text(tr!("Later tab")).clicked() {
                        l.move_in_stack(id, m, 1);
                    }
                    if spai_ui::widgets::icon_button(ui, icon::ARROW_SQUARE_OUT).on_hover_text(tr!("Take out of the stack")).clicked() {
                        l.unstack(m, screen);
                        st.selected = Some(m.clone());
                    }
                    ui.label(l.label(m));
                });
            }
        });
        if !menu {
            r.response.on_hover_text(tr!("Order the tabs, or take one out of the stack"));
        }
    }
    if done && menu {
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(id: &str, rect: [i32; 4]) -> Shown {
        Shown { id: id.into(), rect, members: vec![id.into()], stacked: false, minimized: false, locked: false }
    }

    #[test]
    fn the_smallest_window_under_the_pointer_is_grabbed() {
        let all = vec![shown("big", [0, 0, 800, 600]), shown("small", [100, 100, 200, 200])];
        assert_eq!(grab_at(&all, [150, 200], GRAB), Some((1, Edges::default())));
        assert_eq!(grab_at(&all, [500, 300], GRAB), Some((0, Edges::default())));
        assert_eq!(grab_at(&all, [900, 300], GRAB), None);
        let (_, e) = grab_at(&all, [102, 298], GRAB).unwrap();
        assert_eq!(e, Edges { left: true, bottom: true, ..Edges::default() });
        // Just outside still reaches the edge, which is where a pointer aiming at it lands.
        let (i, e) = grab_at(&all, [303, 200], GRAB).unwrap();
        assert_eq!((i, e), (1, Edges { right: true, ..Edges::default() }));
    }

    #[test]
    fn a_locked_window_offers_no_edges() {
        let mut w = shown("w", [0, 0, 400, 400]);
        w.locked = true;
        assert_eq!(grab_at(&[w], [2, 2], GRAB), Some((0, Edges::default())));
    }

    #[test]
    fn moving_stays_on_screen() {
        let s = [1920, 1080];
        assert_eq!(drag_rect([100, 100, 300, 200], Edges::default(), [50, -30], s), [150, 70, 300, 200]);
        assert_eq!(drag_rect([100, 100, 300, 200], Edges::default(), [-500, 5000], s), [0, 880, 300, 200]);
    }

    #[test]
    fn resizing_keeps_the_minimum_and_the_far_edge() {
        let s = [1920, 1080];
        let left = Edges { left: true, ..Edges::default() };
        assert_eq!(drag_rect([100, 100, 300, 200], left, [50, 0], s), [150, 100, 250, 200]);
        assert_eq!(drag_rect([100, 100, 300, 200], left, [900, 0], s), [400 - MIN_SIZE, 100, MIN_SIZE, 200]);
        let corner = Edges { right: true, bottom: true, ..Edges::default() };
        assert_eq!(drag_rect([100, 100, 300, 200], corner, [5000, 5000], s), [100, 100, 1820, 980]);
        assert_eq!(drag_rect([100, 100, 300, 200], corner, [-5000, -5000], s), [100, 100, MIN_SIZE, MIN_SIZE]);
    }

    #[test]
    fn moved_windows_snap_to_neighbours_and_screen_edges() {
        let s = [1920, 1080];
        let others = [[500, 0, 300, 400]];
        // Right edge 6 px short of the neighbour's left edge.
        assert_eq!(snap([194, 50, 300, 200], Edges::default(), &others, s, SNAP), [200, 50, 300, 200]);
        // Left edge near the neighbour's right edge, top near the screen top.
        assert_eq!(snap([805, 7, 300, 200], Edges::default(), &others, s, SNAP), [800, 0, 300, 200]);
        assert_eq!(snap([1000, 500, 300, 200], Edges::default(), &others, s, SNAP), [1000, 500, 300, 200]);
        assert_eq!(snap([1625, 875, 300, 200], Edges::default(), &others, s, SNAP), [1620, 880, 300, 200]);
    }

    #[test]
    fn resized_edges_snap_and_others_stay() {
        let s = [1920, 1080];
        let others = [[500, 0, 300, 400]];
        let right = Edges { right: true, ..Edges::default() };
        assert_eq!(snap([100, 50, 395, 200], right, &others, s, SNAP), [100, 50, 400, 200]);
        let top = Edges { top: true, ..Edges::default() };
        assert_eq!(snap([100, 395, 300, 200], top, &others, s, SNAP), [100, 400, 300, 195]);
    }

    #[test]
    fn hud_and_targets_sit_where_the_game_draws_them() {
        assert_eq!(hud_rect(0, [1920, 1080]), [750, 930, 420, 150]);
        assert_eq!(hud_rect(-200, [1920, 1080])[0], 550);
        let t = targets_rect([0.68, 0.014], [1920, 1080]);
        assert_eq!(t[0] + t[2], 1305 + TARGET[0]);
        assert_eq!(t[1], 15);
    }
}
