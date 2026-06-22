//! The "Neocom" navigation rail (docs/DESIGN.md §6.1).
//!
//! A vertical list of views on the left edge. Collapsed = icon-only (narrow);
//! expanded = icon + left-aligned label with an accent bar on the active row.
//! Icons come from the Phosphor icon font (loaded in `app::SpaiApp::new`).

use egui_phosphor::regular as icon;
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

    pub fn icon(self) -> &'static str {
        match self {
            View::Dashboard => icon::SQUARES_FOUR,
            View::Map => icon::MAP_TRIFOLD,
            View::Intel => icon::BROADCAST,
            View::Characters => icon::USERS,
            View::Alerts => icon::BELL,
        }
    }
}

pub const WIDTH_COLLAPSED: f32 = 56.0;
pub const WIDTH_EXPANDED: f32 = 196.0;

const ROW_HEIGHT: f32 = 38.0;

/// Render the rail. Returns the (possibly changed) selected view.
pub fn rail(
    ui: &mut egui::Ui,
    current: View,
    expanded: &mut bool,
    open_settings: &mut bool,
) -> View {
    let mut selected = current;
    let accent = ui.visuals().hyperlink_color;
    let weak = ui.visuals().weak_text_color();

    // --- Brand + collapse/expand toggle ---
    ui.add_space(12.0);
    if *expanded {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new(format!("{}  EVE SPAI", icon::DETECTIVE))
                    .color(accent)
                    .strong()
                    .size(16.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if icon_button(ui, icon::CARET_LEFT, weak)
                    .on_hover_text("Collapse")
                    .clicked()
                {
                    *expanded = false;
                }
            });
        });
    } else {
        ui.vertical_centered(|ui| {
            if icon_button(ui, icon::LIST, accent)
                .on_hover_text("Expand")
                .clicked()
            {
                *expanded = true;
            }
        });
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    // --- Primary views ---
    for &v in View::primary() {
        if nav_item(ui, v.icon(), v.label(), v == selected, *expanded) {
            selected = v;
        }
        ui.add_space(4.0);
    }

    // --- Settings pinned to the bottom ---
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.add_space(10.0);
        if nav_item(ui, icon::GEAR_SIX, "Settings", false, *expanded) {
            *open_settings = true;
        }
        ui.add_space(8.0);
        ui.separator();
    });

    selected
}

/// A borderless icon-only button.
fn icon_button(ui: &mut egui::Ui, glyph: &str, color: egui::Color32) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(glyph).color(color).size(18.0)).frame(false))
}

/// A full-width, left-aligned navigation row with hover + active states, drawn by
/// hand so we control alignment, the accent bar, and density.
fn nav_item(ui: &mut egui::Ui, glyph: &str, label: &str, active: bool, expanded: bool) -> bool {
    let accent = ui.visuals().hyperlink_color;
    let normal = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let hover_bg = ui.visuals().widgets.hovered.weak_bg_fill;

    let width = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), egui::Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter().clone();

    if active {
        painter.rect_filled(rect, 5.0, accent.gamma_multiply(0.16));
        let bar = egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 3.0, rect.bottom()));
        painter.rect_filled(bar, 0.0, accent);
    } else if hovered {
        painter.rect_filled(rect, 5.0, hover_bg);
    }

    let color = if active {
        accent
    } else if hovered {
        normal
    } else {
        weak
    };
    let cy = rect.center().y;

    if expanded {
        painter.text(
            egui::pos2(rect.left() + 22.0, cy),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(18.0),
            color,
        );
        painter.text(
            egui::pos2(rect.left() + 48.0, cy),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.5),
            color,
        );
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(18.0),
            color,
        );
    }

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if expanded {
        resp.clicked()
    } else {
        resp.on_hover_text(label).clicked()
    }
}
