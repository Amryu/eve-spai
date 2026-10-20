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

/// What is being dragged from the browser or a list: one type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragItem(pub TypeId);

/// The whole width at `h`, highlighted while the pointer is over it or its menu is open; the
/// response is the row's own (clicks between the cells, right clicks, drops).
pub fn row(ui: &mut egui::Ui, list: &str, id: impl std::hash::Hash, h: f32, selected: bool, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<RowLog>(egui::Id::new(LOG)).0.push((egui::Id::new(list), rect)));
    // Scrolled out of sight, or cut by the fold: the space is kept, the cells are not built, so
    // nothing of them reaches under whatever sits below the list.
    let clip = ui.clip_rect();
    if rect.min.y < clip.min.y - 0.5 || rect.max.y > clip.max.y + 0.5 {
        return resp;
    }
    let over = ui.rect_contains_pointer(rect) && ui.ctx().dragged_id().is_none();
    let dropping = ui.rect_contains_pointer(rect) && egui::DragAndDrop::has_payload_of_type::<DragItem>(ui.ctx());
    if dropping {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().selection.bg_fill.gamma_multiply(0.5));
    } else if selected {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().selection.bg_fill.gamma_multiply(0.35));
    } else if over || resp.context_menu_opened() {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let inner = rect.shrink2(egui::vec2(GAP, 0.0));
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt(id).max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.spacing_mut().item_spacing.x = GAP;
    add(&mut child);
    resp
}

/// A cell of fixed width holding a truncated label, the whole text on hover.
pub fn cell(ui: &mut egui::Ui, width: f32, text: egui::RichText, full: &str) -> egui::Response {
    let r = ui
        .allocate_ui_with_layout(egui::vec2(width, ROW_H - 4.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.add(egui::Label::new(text).truncate().selectable(false))
        })
        .inner;
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
