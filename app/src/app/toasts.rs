//! Short notices in the window's bottom-right corner, over the content: what just happened, gone
//! after a few seconds, so nothing is pushed down to make room for it.

use super::*;

const SHOWN: std::time::Duration = std::time::Duration::from_secs(4);
const SHOWN_ERROR: std::time::Duration = std::time::Duration::from_secs(10);
const MAX_W: f32 = 420.0;

pub(crate) struct Toast {
    text: String,
    error: bool,
    until: std::time::Instant,
    /// A toast with a key replaces the one before it with the same key.
    key: Option<&'static str>,
    /// Shown with a spinner until replaced or it runs out.
    busy: bool,
}

impl SpaiApp {
    pub(crate) fn toast(&mut self, text: impl Into<String>) {
        self.push_toast(None, text.into(), false, false, SHOWN);
    }

    pub(crate) fn toast_error(&mut self, text: impl Into<String>) {
        self.push_toast(None, text.into(), true, false, SHOWN_ERROR);
    }

    /// Work under way: kept up while it is called again each frame, then replaced by the outcome.
    pub(crate) fn toast_busy(&mut self, key: &'static str, text: impl Into<String>) {
        self.push_toast(Some(key), text.into(), false, true, std::time::Duration::from_millis(500));
    }

    pub(crate) fn toast_keyed(&mut self, key: &'static str, text: impl Into<String>, error: bool) {
        self.push_toast(Some(key), text.into(), error, false, if error { SHOWN_ERROR } else { SHOWN });
    }

    fn push_toast(&mut self, key: Option<&'static str>, text: String, error: bool, busy: bool, for_: std::time::Duration) {
        if key.is_some() {
            self.toasts.retain(|t| t.key != key);
        }
        self.toasts.push(Toast { text, error, until: std::time::Instant::now() + for_, key, busy });
    }

    pub(crate) fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let mut dismiss = None;
        egui::Area::new(egui::Id::new("toasts"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -36.0))
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(MAX_W);
                for (i, t) in self.toasts.iter().enumerate() {
                    let colour = if t.error { crate::theme::standing::WARNING } else { ui.visuals().text_color() };
                    let r = egui::Frame::popup(ui.style())
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if t.busy {
                                    ui.spinner();
                                }
                                // One line: a long notice is cut, the whole on hover.
                                ui.add(egui::Label::new(egui::RichText::new(&t.text).color(colour)).truncate()).on_hover_text(&t.text);
                            });
                        })
                        .response
                        .interact(egui::Sense::click())
                        .on_hover_text("Click to dismiss");
                    if r.clicked() {
                        dismiss = Some(i);
                    }
                    ui.add_space(4.0);
                }
            });
        if let Some(i) = dismiss {
            self.toasts.remove(i);
        }
        let next = self.toasts.iter().map(|t| t.until).min().unwrap_or(now);
        ctx.request_repaint_after(next.saturating_duration_since(now));
    }
}
