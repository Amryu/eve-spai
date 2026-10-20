//! Type icons from EVE's image server; both hosts cache them (the desktop on disk, the browser in
//! its own cache).

use spai_fit::ids::TypeId;

pub fn type_icon_uri(type_id: TypeId) -> String {
    format!("https://images.evetech.net/types/{type_id}/icon?size=32")
}

pub fn ship_render_uri(type_id: TypeId) -> String {
    format!("https://images.evetech.net/types/{type_id}/render?size=64")
}

/// An icon the size of a text line, with the type's name on hover.
pub fn type_icon(ui: &mut egui::Ui, type_id: TypeId, size: f32) -> egui::Response {
    ui.add(egui::Image::new(type_icon_uri(type_id)).fit_to_exact_size(egui::vec2(size, size)).corner_radius(3.0))
}
