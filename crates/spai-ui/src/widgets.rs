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
