//! A list row that lights up under the pointer, takes a right click for its menu and a drop,
//! and lays its cells out left to right at a fixed height.

use spai_fit::ids::TypeId;

pub const ROW_H: f32 = 24.0;
/// Horizontal gap between cells and the row's side padding.
pub const GAP: f32 = 6.0;

/// Every row laid out this frame, with the list it belongs to: the layout checks read it to see
/// that rows of one list keep one height and one pitch.
#[derive(Clone, Default)]
pub struct RowLog(pub Vec<(egui::Id, egui::Rect)>);

const LOG: &str = "sim_row_log";

pub fn clear_log(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(LOG), RowLog::default()));
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(CUT), CutRows::default()));
}

/// Rows partly scrolled out of view this frame: the whole row, and the part that shows. What is
/// inside them is clipped to that part, which the accessibility tree does not say.
#[derive(Clone, Default)]
pub struct CutRows(pub Vec<(egui::Rect, egui::Rect)>);

const CUT: &str = "sim_cut_rows";

pub fn cut_rows(ctx: &egui::Context) -> Vec<(egui::Rect, egui::Rect)> {
    ctx.data(|d| d.get_temp::<CutRows>(egui::Id::new(CUT)).map(|c| c.0).unwrap_or_default())
}

pub fn log(ctx: &egui::Context) -> Vec<(egui::Id, egui::Rect)> {
    ctx.data(|d| d.get_temp::<RowLog>(egui::Id::new(LOG)).map(|l| l.0).unwrap_or_default())
}

/// Lists whose rows differ in height, or sit at uneven pitch, as sentences; empty when fine.
pub fn uneven_lists(log: &[(egui::Id, egui::Rect)]) -> Vec<String> {
    let mut by_list: Vec<(egui::Id, Vec<egui::Rect>)> = Vec::new();
    for (id, r) in log {
        match by_list.iter_mut().find(|(i, _)| i == id) {
            Some((_, v)) => v.push(*r),
            None => by_list.push((*id, vec![*r])),
        }
    }
    let mut out = Vec::new();
    for (id, mut rects) in by_list {
        rects.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
        let h = rects[0].height();
        if let Some(r) = rects.iter().find(|r| (r.height() - h).abs() > 0.5) {
            out.push(format!("{id:?}: a row is {:.1} high where the first is {h:.1}", r.height()));
            continue;
        }
        // Rows side by side (a dock with two panels) sit at one y; only rows below each other
        // have a pitch.
        let pitches: Vec<f32> = rects.windows(2).map(|w| w[1].min.y - w[0].min.y).filter(|p| *p > 0.5).collect();
        if let (Some(first), Some(odd)) = (pitches.first(), pitches.iter().find(|p| (*p - pitches[0]).abs() > 0.5)) {
            out.push(format!("{id:?}: rows {first:.1} apart, then {odd:.1}"));
        }
    }
    out
}

/// A fitted module being dragged to another slot of its rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragModule(pub spai_fit::fit::Slot, pub u8);

/// Starts carrying `payload` when the primary button drags `resp`, and remembers what to show by
/// the pointer meanwhile. A right click never starts one, so context menus keep working, and
/// nothing on the page moves with the pointer, so the buttons under it stay where they are. The
/// cursor changes only once something is being carried.
pub fn drag_from<P: std::any::Any + Send + Sync>(ui: &egui::Ui, resp: &egui::Response, payload: P, label: &str) {
    if resp.dragged_by(egui::PointerButton::Primary) && !egui::DragAndDrop::has_any_payload(ui.ctx()) && is_a_drag(ui.ctx()) {
        egui::DragAndDrop::set_payload(ui.ctx(), payload);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(GHOST), label.to_owned()));
    }
}

const GHOST: &str = "sim_drag_label";

/// What was dropped on `resp`, when it is a `P`. egui's own release takes the payload whatever its
/// type, so asking for the wrong one would throw away a drop meant for another handler.
pub fn released<P: std::any::Any + Send + Sync>(resp: &egui::Response) -> Option<std::sync::Arc<P>> {
    if egui::DragAndDrop::has_payload_of_type::<P>(&resp.ctx) {
        resp.dnd_release_payload::<P>()
    } else {
        None
    }
}

/// How far the pointer has to travel, and how long the button be held, before a press is a drag:
/// a click that slips a few pixels stays a click.
pub const DRAG_DISTANCE: f32 = 6.0;
pub const DRAG_DELAY_S: f64 = 0.06;

/// Whether the press going on is a drag by those measures.
pub fn is_drag(origin: egui::Pos2, now: egui::Pos2, held_s: f64) -> bool {
    origin.distance(now) >= DRAG_DISTANCE && held_s >= DRAG_DELAY_S
}

fn is_a_drag(ctx: &egui::Context) -> bool {
    ctx.input(|i| match (i.pointer.press_origin(), i.pointer.latest_pos(), i.pointer.press_start_time()) {
        (Some(o), Some(p), Some(t)) => is_drag(o, p, i.time - t),
        _ => false,
    })
}

/// The name of what is being carried, by the pointer, while a drag is on.
pub fn drag_ghost(ctx: &egui::Context) {
    let carrying = egui::DragAndDrop::has_payload_of_type::<DragItem>(ctx) || egui::DragAndDrop::has_payload_of_type::<DragModule>(ctx);
    if !carrying {
        return;
    }
    let (Some(label), Some(pos)) = (ctx.data(|d| d.get_temp::<String>(egui::Id::new(GHOST))), ctx.pointer_latest_pos()) else { return };
    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
    egui::Area::new(egui::Id::new("sim_drag_ghost")).order(egui::Order::Tooltip).fixed_pos(pos + egui::vec2(14.0, 10.0)).interactable(false).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.label(label);
        });
    });
}

/// What is being dragged from the browser or a list: one type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragItem(pub TypeId);

/// The whole width at `h`, highlighted while the pointer is over it or its menu is open; the
/// response is the row's own (clicks between the cells, right clicks, drops).
pub fn row(ui: &mut egui::Ui, list: &str, id: impl std::hash::Hash, h: f32, selected: bool, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    row_tinted(ui, list, id, h, selected, None, add)
}

/// A row in a state that should show at a glance (an active or overloaded module): a faint wash
/// of `tint`, or, selected, the selection's own colour with a bar of the tint at its left edge,
/// so neither hides the other.
pub fn row_tinted(ui: &mut egui::Ui, list: &str, id: impl std::hash::Hash, h: f32, selected: bool, tint: Option<egui::Color32>, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let w = ui.available_width();
    // Drag too, so a row can be carried by its name; `drag_from` decides which rows carry anything.
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click_and_drag());
    ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<RowLog>(egui::Id::new(LOG)).0.push((egui::Id::new(list), rect)));
    // Out of sight altogether: the space is kept, the cells are not built. A row partly in view
    // is drawn, clipped at the edge, so it scrolls away smoothly instead of vanishing early.
    let clip = ui.clip_rect();
    if !rect.intersects(clip) {
        return resp;
    }
    let visible = clip.intersect(rect);
    if visible != rect {
        ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<CutRows>(egui::Id::new(CUT)).0.push((rect, visible)));
    }
    let over = ui.rect_contains_pointer(rect) && ui.ctx().dragged_id().is_none();
    let dropping = ui.rect_contains_pointer(rect) && egui::DragAndDrop::has_payload_of_type::<DragItem>(ui.ctx());
    if let Some(t) = tint.filter(|_| !selected) {
        ui.painter().rect_filled(rect, 3.0, t.gamma_multiply(0.12));
    }
    if dropping {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().selection.bg_fill.gamma_multiply(0.5));
    } else if selected {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().selection.bg_fill.gamma_multiply(0.35));
        if let Some(t) = tint {
            ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), 1.5, t);
        }
    } else if over || resp.context_menu_opened() {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let inner = rect.shrink2(egui::vec2(GAP, 0.0));
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.set_clip_rect(clip.intersect(rect));
    child.spacing_mut().item_spacing.x = GAP;
    add(&mut child);
    resp
}

/// A cell of fixed width holding a truncated label, the whole text on hover. The response is the
/// whole cell's, so a tooltip shows anywhere over it, not only over the letters.
pub fn cell(ui: &mut egui::Ui, width: f32, text: egui::RichText, full: &str) -> egui::Response {
    let r = ui
        .allocate_ui_with_layout(egui::vec2(width, ROW_H - 4.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            // The cell carries the one tooltip; the label's own for elided text would be a second.
            ui.add(egui::Label::new(text).truncate().selectable(false).show_tooltip_when_elided(false));
        })
        .response;
    if full.is_empty() {
        r
    } else {
        r.on_hover_text(full)
    }
}

/// A right-aligned number cell.
pub fn num_cell(ui: &mut egui::Ui, width: f32, text: impl Into<String>, weak: bool) {
    let t = egui::RichText::new(text.into()).monospace();
    num_cell_rich(ui, width, if weak { t.weak() } else { t });
}

/// A right-aligned number cell in a colour.
pub fn num_cell_colored(ui: &mut egui::Ui, width: f32, text: impl Into<String>, color: egui::Color32) {
    num_cell_rich(ui, width, egui::RichText::new(text.into()).monospace().color(color));
}

fn num_cell_rich(ui: &mut egui::Ui, width: f32, t: egui::RichText) {
    ui.allocate_ui_with_layout(egui::vec2(width, ROW_H - 4.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        ui.add(egui::Label::new(t).truncate().selectable(false));
    });
}

/// A column header cell.
pub fn head_cell(ui: &mut egui::Ui, width: f32, text: &str, right: bool) {
    head_cell_rich(ui, width, egui::RichText::new(text).weak(), right);
}

/// A column header cell in a colour.
pub fn head_cell_colored(ui: &mut egui::Ui, width: f32, text: &str, right: bool, color: egui::Color32) {
    head_cell_rich(ui, width, egui::RichText::new(text).color(color), right);
}

fn head_cell_rich(ui: &mut egui::Ui, width: f32, t: egui::RichText, right: bool) {
    let layout = if right { egui::Layout::right_to_left(egui::Align::Center) } else { egui::Layout::left_to_right(egui::Align::Center) };
    ui.allocate_ui_with_layout(egui::vec2(width, 18.0), layout, |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        ui.add(egui::Label::new(t).truncate().selectable(false));
    });
}

/// A section heading at a steady height, with room for buttons on its right.
pub fn heading(ui: &mut egui::Ui, title: &str, note: Option<egui::RichText>, right: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add_space(GAP);
        ui.add(egui::Label::new(egui::RichText::new(title).strong()).selectable(false));
        if let Some(n) = note {
            ui.add(egui::Label::new(n).selectable(false));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(GAP);
            right(ui);
        });
    });
}

/// Width of one trailing control slot: an icon button, or a short number.
pub const SLOT: f32 = 22.0;

/// The width of `n` slots side by side, the gaps between them included.
pub fn slots_w(n: f32) -> f32 {
    n * SLOT + (n - 1.0).max(0.0) * GAP
}

/// A control in a slot of fixed width, `n` slots wide, its contents centred. Rows that end in
/// slots end at the same right edge whatever sits in them, and a slot left empty keeps the column.
pub fn slot<R>(ui: &mut egui::Ui, n: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let w = slots_w(n);
    ui.allocate_ui_with_layout(egui::vec2(w, ROW_H - 4.0), egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center), |ui| {
        ui.set_min_width(w);
        ui.set_max_width(w);
        add(ui)
    })
    .inner
}

/// An empty slot, keeping the column for a control this row does not have.
pub fn no_slot(ui: &mut egui::Ui) {
    ui.allocate_exact_size(egui::vec2(SLOT, ROW_H - 4.0), egui::Sense::hover());
}

/// What the name cell may take when `slots` slots follow it.
pub fn name_w(ui: &egui::Ui, slots: f32) -> f32 {
    (ui.available_width() - slots_w(slots) - GAP).max(40.0)
}
