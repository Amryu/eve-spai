//! The "Neocom" navigation rail (docs/DESIGN.md §6.1).
//!
//! A vertical list of views on the left edge. Collapsed = icon-only (narrow);
//! expanded = icon + label. For M0 the "icon" is the view's letter glyph in an
//! accent-tinted slot; real icons (egui_phosphor) are a later polish item.

use serde::{Deserialize, Serialize};

/// A top-level destination shown in the nav rail.
///
/// Only the Essential (MVP) views exist so far; Advanced views (Assets, Wallet,
/// Comms, …) will be appended here as they are built (docs/DESIGN.md §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    Dashboard,
    Map,
    Intel,
    Characters,
    Alerts,
}

impl View {
    /// Primary views, in rail order.
    pub fn primary() -> &'static [View] {
        &[
            View::Dashboard,
            View::Map,
            View::Intel,
            View::Characters,
            View::Alerts,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            View::Dashboard => "Overview",
            View::Map => "Map",
            View::Intel => "Intel",
            View::Characters => "Characters",
            View::Alerts => "Alerts",
        }
    }

    /// Placeholder glyph until an icon font is wired up.
    pub fn glyph(self) -> &'static str {
        match self {
            View::Dashboard => "O",
            View::Map => "M",
            View::Intel => "I",
            View::Characters => "C",
            View::Alerts => "A",
        }
    }
}

pub const WIDTH_COLLAPSED: f32 = 52.0;
pub const WIDTH_EXPANDED: f32 = 188.0;

/// Render the rail. Returns the (possibly changed) selected view.
pub fn rail(
    ui: &mut egui::Ui,
    current: View,
    expanded: &mut bool,
    open_settings: &mut bool,
) -> View {
    let mut selected = current;
    ui.add_space(6.0);

    // Expand / collapse toggle.
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        let toggle = if *expanded { "\u{00AB} Collapse" } else { "\u{00BB}" };
        if ui.button(toggle).on_hover_text("Toggle labels").clicked() {
            *expanded = !*expanded;
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    for &v in View::primary() {
        if nav_button(ui, v, v == selected, *expanded) {
            selected = v;
        }
        ui.add_space(2.0);
    }

    // Secondary actions pinned to the bottom.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.add_space(6.0);
        let label = if *expanded { "S   Settings" } else { "S" };
        if ui
            .add_sized([ui.available_width(), 32.0], egui::Button::new(label))
            .clicked()
        {
            *open_settings = true;
        }
        ui.separator();
    });

    selected
}

fn nav_button(ui: &mut egui::Ui, view: View, active: bool, expanded: bool) -> bool {
    let text = if expanded {
        format!("{}   {}", view.glyph(), view.label())
    } else {
        view.glyph().to_string()
    };
    let widget = egui::Button::selectable(active, egui::RichText::new(text).size(15.0));
    let resp = ui
        .add_sized([ui.available_width(), 34.0], widget)
        .on_hover_text(view.label());
    resp.clicked()
}
