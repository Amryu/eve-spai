//! EVE Spai — entry point.
//!
//! M0 scaffold: a single-window egui app with a collapsible "Neocom" nav rail,
//! a three-colour theme engine, a settings dialog, and SQLite-backed persistence.
//! No EVE data yet — views are placeholders (see docs/DESIGN.md §10, milestone M0).

mod app;
mod nav;
mod sde;
mod settings;
mod store;
mod theme;
mod views;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EVE Spai")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([720.0, 460.0]),
        ..Default::default()
    };

    eframe::run_native(
        "EVE Spai",
        native_options,
        Box::new(|cc| Ok(Box::new(app::SpaiApp::new(cc)))),
    )
}
