//! Dropdowns of a fixed width.

/// A dropdown exactly `width` wide. egui takes a combo box's width as a minimum and lays the
/// choice out against all the room there is, so a long one widened the box; here it is cut to the
/// box, whole on hover, and the menu is at least as wide.
pub fn combo<R>(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, width: f32, selected: impl Into<egui::RichText>, menu: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>> {
    let selected: egui::RichText = selected.into();
    let full = selected.text().to_owned();
    let height = ui.spacing().interact_size.y;
    let mut r = ui
        .allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_max_width(width);
            egui::ComboBox::from_id_salt(id_salt).width(width).truncate().selected_text(selected).show_ui(ui, menu)
        })
        .inner;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let text_w = ui.painter().layout_no_wrap(full.clone(), font, egui::Color32::WHITE).size().x;
    let room = width - 2.0 * ui.spacing().button_padding.x - ui.spacing().icon_width - ui.spacing().icon_spacing;
    if text_w > room {
        r.response = r.response.on_hover_text(full);
    }
    r
}
