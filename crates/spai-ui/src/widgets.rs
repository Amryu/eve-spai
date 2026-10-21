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
        // At the row's top edge in a rect of exactly `size`. Left to the row, each label is centred
        // on the row's height so far, and a row of them steps down a pixel per label.
        let size: egui::Vec2 = size.into();
        let rect = egui::Rect::from_min_size(self.cursor().min, size);
        self.advance_cursor_after_rect(rect);
        self.place(
            rect,
            egui::Button::new(text)
                .selected(selected)
                .frame_when_inactive(selected)
                .stroke(egui::Stroke::NONE)
                .wrap_mode(egui::TextWrapMode::Truncate)
                .min_size(rect.size()),
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

/// "Copy name" for a menu that is already open: names are pasted into the game, chat and other tools.
pub fn copy_item(ui: &mut egui::Ui, text: &str) {
    if ui.button(crate::trf!("{icon}  Copy name", icon = egui_phosphor::regular::COPY)).on_hover_text(text).clicked() {
        ui.ctx().copy_text(text.to_owned());
        ui.close();
    }
}

/// A right-click menu holding only [`copy_item`], for names that have no menu of their own. egui allows one
/// menu per response, so a widget with a menu calls `copy_item` inside it instead.
pub fn copy_menu(resp: &egui::Response, text: &str) {
    resp.context_menu(|ui| copy_item(ui, text));
}

/// Height of a tab strip's tabs.
pub const TAB_H: f32 = 24.0;
const TAB_PAD_X: f32 = 8.0;
const TAB_GAP: f32 = 6.0;
const TAB_CLOSE_W: f32 = 16.0;

/// What a tab reported this frame; `resp` hosts the context menu.
pub struct TabHit {
    pub select: bool,
    pub close: bool,
    pub resp: egui::Response,
}

/// One compact, flush tab in the Jabber style: fixed height, the name, and on hover or when
/// selected a close X at its right end, where an unsaved tab otherwise shows a dot. Call it in
/// a horizontal row with zero item spacing.
/// The width `tab_box` takes for the label, to plan a row of tabs before drawing them.
pub fn tab_box_width(ui: &egui::Ui, label: &str) -> f32 {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let text_w = ui.painter().layout_no_wrap(label.to_owned(), body, egui::Color32::WHITE).size().x;
    TAB_PAD_X + text_w + TAB_GAP + TAB_CLOSE_W + TAB_PAD_X
}

pub fn tab_box(ui: &mut egui::Ui, id: egui::Id, selected: bool, label: &str, marked: bool) -> TabHit {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let w = tab_box_width(ui, label);
    let (_, rect) = ui.allocate_space(egui::vec2(w, TAB_H));
    let resp = ui.interact(rect, id, egui::Sense::click());
    // Geometric, not `hovered()`: the X is a second widget inside the rect and would otherwise
    // take the hover away and flicker.
    let hovered = resp.contains_pointer();
    let (fill, text_color, accent, sep_col, weak_col, strong_col) = {
        let v = ui.visuals();
        (
            if selected {
                v.selection.bg_fill
            } else if hovered {
                v.widgets.hovered.weak_bg_fill
            } else {
                egui::Color32::TRANSPARENT
            },
            if selected { v.strong_text_color() } else { v.text_color() },
            v.selection.stroke.color,
            v.widgets.noninteractive.bg_stroke.color,
            v.weak_text_color(),
            v.strong_text_color(),
        )
    };
    let painter = ui.painter().clone();
    if fill != egui::Color32::TRANSPARENT {
        painter.rect_filled(rect, 0.0, fill);
    }
    if selected {
        painter.hline(rect.x_range(), rect.top() + 1.0, egui::Stroke::new(2.0, accent));
    }
    painter.vline(rect.right(), rect.y_range(), egui::Stroke::new(1.0, sep_col));
    let cy = rect.center().y;
    let galley = painter.layout_no_wrap(label.to_owned(), body, text_color);
    painter.galley(egui::pos2(rect.left() + TAB_PAD_X, cy - galley.size().y / 2.0), galley, text_color);
    let slot = egui::Rect::from_center_size(egui::pos2(rect.right() - TAB_PAD_X - TAB_CLOSE_W / 2.0, cy), egui::vec2(TAB_CLOSE_W, TAB_CLOSE_W));
    let mut close = false;
    if hovered || selected {
        let x = ui.interact(slot, id.with("close"), egui::Sense::click());
        let col = if x.hovered() { strong_col } else { weak_col };
        painter.text(slot.center(), egui::Align2::CENTER_CENTER, egui_phosphor::regular::X, egui::FontId::proportional(13.0), col);
        close = x.clicked();
    } else if marked {
        painter.circle_filled(slot.center(), 3.0, weak_col);
    }
    TabHit { select: resp.clicked() && !close, close: close || resp.middle_clicked(), resp }
}
