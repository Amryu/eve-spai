//! Small widgets and text helpers both apps use.

/// `selectable_label` and `selectable_value`, minus the border egui adds under the cursor.
///
/// An unframed button gets a stroke when it is hovered, and the stroke counts towards its size, so
/// every widget after it on the row steps sideways as the pointer passes. Dropping the stroke
/// keeps the geometry identical in both states and looks the same: the hover fill is what reads as
/// hover, not a one-pixel outline.
pub trait SteadySelect {
    fn menu_label<'a>(&mut self, selected: bool, text: impl egui::IntoAtoms<'a>)
        -> egui::Response;

    fn menu_value<'a, V: PartialEq>(
        &mut self,
        current: &mut V,
        value: V,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response;

    /// The same, at a fixed size, for a row of tabs that must not move as the pointer crosses it.
    fn menu_label_sized<'a>(
        &mut self,
        size: impl Into<egui::Vec2>,
        selected: bool,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response;
}

/// A small button holding only an icon: about as wide as it is tall, where the theme's padding,
/// sized for words, would make it twice that.
pub fn icon_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
        ui.add(egui::Button::new(text).small())
    })
    .inner
}

impl SteadySelect for egui::Ui {
    fn menu_label<'a>(
        &mut self,
        selected: bool,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        self.add(
            egui::Button::new(text)
                .selected(selected)
                .frame_when_inactive(selected)
                .stroke(egui::Stroke::NONE),
        )
    }

    fn menu_label_sized<'a>(
        &mut self,
        size: impl Into<egui::Vec2>,
        selected: bool,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        self.add_sized(
            size,
            egui::Button::new(text)
                .selected(selected)
                .frame_when_inactive(selected)
                .stroke(egui::Stroke::NONE),
        )
    }

    fn menu_value<'a, V: PartialEq>(
        &mut self,
        current: &mut V,
        value: V,
        text: impl egui::IntoAtoms<'a>,
    ) -> egui::Response {
        let mut resp = self.menu_label(*current == value, text);
        if resp.clicked() && *current != value {
            *current = value;
            resp.mark_changed();
        }
        resp
    }
}

/// "45s", "12m", "3h", "9d": one unit, for ages that sit in a narrow column.
pub fn human_ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86_400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86_400)
    }
}

/// When a signature was first seen, as a clock time, with the weekday when not today: how long a
/// hole has been open starts there.
pub fn found_at(at: i64, now: i64, _eve: bool) -> String {
    // How long ago, which is what matters for a signature; the time itself is on hover.
    crate::trf!("{age} ago", age = human_ago(now - at))
}

/// A signature's kind as a leading icon, so the two read apart at a glance: a magnifying glass for
/// a signature, which takes probing, a sparkle for an anomaly, which the scanner shows at once.
pub fn sig_icon(kind: &str) -> &'static str {
    if kind.to_lowercase().contains("anomal") {
        egui_phosphor::regular::SPARKLE
    } else {
        egui_phosphor::regular::MAGNIFYING_GLASS
    }
}

/// [`found_at`] for a hover: the date too, and how long ago.
pub fn found_hover(at: i64, now: i64, eve: bool) -> String {
    let Some(t) = chrono::DateTime::from_timestamp(at, 0) else { return String::new() };
    let when = if eve { format!("{} EVE", t.format("%Y-%m-%d %H:%M")) } else { t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string() };
    crate::trf!("First seen {when}, {ago} ago", when = when, ago = human_ago(now - at))
}

/// Unseen in a paste for over a day: yellow; over three: grey.
pub fn age_color(visuals: &egui::Visuals, now: i64, seen: i64) -> Option<egui::Color32> {
    match now - seen {
        a if a > 3 * 86_400 => Some(visuals.weak_text_color()),
        a if a > 86_400 => Some(crate::theme::standing::WARNING),
        _ => None,
    }
}
