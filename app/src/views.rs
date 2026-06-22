//! View content. M0 ships placeholders — each becomes a real Essential feature in
//! later milestones (docs/DESIGN.md §7.1). The shell already routes to them so the
//! nav rail, theming, and layout can be exercised end-to-end.

use crate::nav::View;

pub fn show(ui: &mut egui::Ui, view: View) {
    ui.add_space(10.0);

    match view {
        View::Dashboard => placeholder(
            ui,
            "Overview",
            "At-a-glance situational summary: active alerts, nearby hostiles, \
             tracked-character locations. (Milestone M2+.)",
        ),
        // View::Map is rendered by SpaiApp::map_view (it needs app state).
        View::Map => {}
        // Intel and Battles are rendered by SpaiApp (they need app state).
        View::Intel => {}
        View::Battles => {}
        // View::Characters is rendered by SpaiApp::characters_view (it needs app state).
        View::Characters => {}
        View::Alerts => placeholder(
            ui,
            "Alerts",
            "Rules over intel/log events firing sound + desktop notifications. \
             (Milestone M4.)",
        ),
    }
}

fn placeholder(ui: &mut egui::Ui, title: &str, body: &str) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).strong());
        ui.add_space(4.0);
        ui.label(egui::RichText::new(body).weak());
    });
}
