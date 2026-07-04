use egui_phosphor::regular as icon;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    Dashboard,
    Map,
    Intel,
    Battles,
    Wormholes,
    Lookup,
    Characters,
    Alerts,
    Jabber,
    Settings,
}

impl View {
    pub fn primary() -> &'static [View] {
        &[
            View::Dashboard,
            View::Map,
            View::Wormholes,
            View::Intel,
            View::Alerts,
            View::Battles,
            View::Lookup,
            View::Characters,
            View::Jabber,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            View::Dashboard => "Overview",
            View::Map => "Map",
            View::Intel => "Intel",
            View::Battles => "Battles",
            View::Wormholes => "Wormholes",
            View::Lookup => "Lookup",
            View::Characters => "Characters",
            View::Alerts => "Alerts",
            View::Jabber => "Jabber",
            View::Settings => "Settings",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            View::Dashboard => icon::SQUARES_FOUR,
            View::Map => icon::MAP_TRIFOLD,
            View::Intel => icon::BROADCAST,
            View::Battles => icon::SWORD,
            View::Wormholes => icon::SPIRAL,
            View::Lookup => icon::MAGNIFYING_GLASS,
            View::Characters => icon::USERS,
            View::Alerts => icon::BELL,
            View::Jabber => icon::CHAT_TEXT,
            View::Settings => icon::GEAR_SIX,
        }
    }
}

pub const WIDTH_COLLAPSED: f32 = 56.0;
pub const WIDTH_EXPANDED: f32 = 196.0;

const ROW_HEIGHT: f32 = 38.0;

pub fn rail(ui: &mut egui::Ui, current: View, expanded: &mut bool, badges: &[View]) -> View {
    let mut selected = current;
    let accent = ui.visuals().hyperlink_color;
    let weak = ui.visuals().weak_text_color();

    ui.add_space(12.0);
    if *expanded {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new("EVE SPAI")
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

    for &v in View::primary() {
        if nav_item(ui, v.icon(), v.label(), v == selected, *expanded, badges.contains(&v)) {
            selected = v;
        }
        ui.add_space(4.0);
    }

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.add_space(10.0);
        if nav_item(ui, icon::GEAR_SIX, "Settings", selected == View::Settings, *expanded, false) {
            selected = View::Settings;
        }
        ui.add_space(8.0);
        ui.separator();
    });

    selected
}

fn icon_button(ui: &mut egui::Ui, glyph: &str, color: egui::Color32) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(glyph).color(color).size(18.0)).frame(false))
}

fn nav_item(
    ui: &mut egui::Ui,
    glyph: &str,
    label: &str,
    active: bool,
    expanded: bool,
    badge: bool,
) -> bool {
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

    if badge {
        let icon_pos = if expanded { egui::pos2(rect.left() + 22.0, cy) } else { rect.center() };
        painter.circle_filled(
            icon_pos + egui::vec2(9.0, -8.0),
            4.0,
            egui::Color32::from_rgb(0xE0, 0x4C, 0x4C),
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
