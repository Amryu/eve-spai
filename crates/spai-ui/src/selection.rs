//! egui keeps one label selection for the whole context and drops it at the end of any pass that
//! did not draw both of its ends. Every other viewport's pass (pop-outs, the alert window, dialogs,
//! the hidden-window heartbeat) is such a pass, so a selection made in one window was gone a frame
//! later and nothing could be copied. This puts it back after a pass in another viewport, unless
//! that pass had a click, which deselects as it always did.

use egui::text_selection::LabelSelectionState;
use std::collections::HashMap;

#[derive(Default)]
pub struct KeepAcrossViewports {
    /// The viewport whose labels last held the selection.
    owner: Option<egui::ViewportId>,
    /// Per viewport, because an immediate viewport's pass runs inside its parent's.
    before: HashMap<egui::ViewportId, Option<LabelSelectionState>>,
}

fn selection(ctx: &egui::Context) -> Option<LabelSelectionState> {
    ctx.with_plugin(|p: &mut LabelSelectionState| p.has_selection().then(|| p.clone())).flatten()
}

impl egui::Plugin for KeepAcrossViewports {
    fn debug_name(&self) -> &'static str {
        "KeepSelectionAcrossViewports"
    }

    fn on_begin_pass(&mut self, ui: &mut egui::Ui) {
        self.before.insert(ui.ctx().viewport_id(), selection(ui.ctx()));
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let vp = ctx.viewport_id();
        let before = self.before.remove(&vp).flatten();
        if selection(&ctx).is_some() {
            self.owner = Some(vp);
            return;
        }
        let foreign = self.owner.is_some_and(|o| o != vp);
        let clicked = ui.input(|i| i.pointer.any_pressed());
        match before {
            Some(state) if foreign && !clicked => {
                ctx.with_plugin(|p: &mut LabelSelectionState| *p = state);
            }
            _ => self.owner = None,
        }
    }
}

/// Call once per context, after it exists.
pub fn install(ctx: &egui::Context) {
    ctx.add_plugin(KeepAcrossViewports::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(vp: egui::ViewportId, events: Vec<egui::Event>) -> egui::RawInput {
        let mut r = egui::RawInput {
            viewport_id: vp,
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
            events,
            ..Default::default()
        };
        r.viewports.insert(vp, Default::default());
        r
    }

    fn btn(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
    }

    fn has(ctx: &egui::Context) -> bool {
        selection(ctx).is_some()
    }

    /// Select in the main window, then let another window paint: the selection stays and copies.
    /// A click in the other window still clears it.
    #[test]
    fn a_selection_survives_another_viewports_pass_but_not_a_click_there() {
        let ctx = egui::Context::default();
        install(&ctx);
        let root = egui::ViewportId::ROOT;
        let other = egui::ViewportId::from_hash_of("other");
        let copied = std::cell::RefCell::new(String::new());
        let pass = |vp, events| {
            let out = ctx.run_ui(raw(vp, events), |ui| {
                ui.label(if vp == root { "pilot: form up now please" } else { "elsewhere" });
            });
            for c in out.platform_output.commands {
                if let egui::OutputCommand::CopyText(t) = c {
                    *copied.borrow_mut() = t;
                }
            }
        };
        let (a, b) = (egui::pos2(12.0, 10.0), egui::pos2(300.0, 10.0));
        pass(root, vec![egui::Event::PointerMoved(a)]);
        pass(root, vec![btn(a, true)]);
        pass(root, vec![egui::Event::PointerMoved(b)]);
        pass(root, vec![btn(b, false)]);
        assert!(has(&ctx));
        pass(other, vec![]);
        assert!(has(&ctx), "another window's pass must not clear it");
        pass(root, vec![]);
        pass(root, vec![egui::Event::Copy]);
        assert!(copied.borrow().contains("form up"), "{:?}", copied.borrow());
        pass(other, vec![btn(egui::pos2(5.0, 5.0), true)]);
        pass(other, vec![btn(egui::pos2(5.0, 5.0), false)]);
        assert!(!has(&ctx), "a click elsewhere deselects");
    }
}
