//! egui keeps one label selection for the whole context and drops it at the end of any pass that
//! did not draw both of its ends. Every other viewport's pass (pop-outs, the alert window, dialogs,
//! the hidden-window heartbeat) is such a pass, so a selection made in one window was gone a frame
//! later and nothing could be copied. This puts it back after a pass in another viewport, unless
//! that pass had a click, which deselects as it always did.

use egui::text_selection::LabelSelectionState;
use std::collections::HashMap;

#[derive(Default)]
pub struct KeepAcrossViewports {
    ctx: Option<egui::Context>,
    /// The viewport a selection is made in: where the pointer last went down.
    owner: Option<egui::ViewportId>,
    /// The whole state as each pass found it, per viewport, because an immediate viewport's pass
    /// runs inside its parent's. Taken in the input hook: by `on_begin_pass` egui has already
    /// reset the flags that say both ends were drawn, and a nested pass that put back that reset
    /// state cleared the selection at the end of its parent's pass, every frame.
    before: HashMap<egui::ViewportId, LabelSelectionState>,
}

fn state(ctx: &egui::Context) -> Option<LabelSelectionState> {
    ctx.with_plugin(|p: &mut LabelSelectionState| p.clone())
}

fn has_selection(ctx: &egui::Context) -> bool {
    ctx.with_plugin(|p: &mut LabelSelectionState| p.has_selection()).unwrap_or(false)
}

impl egui::Plugin for KeepAcrossViewports {
    fn debug_name(&self) -> &'static str {
        "KeepSelectionAcrossViewports"
    }

    fn setup(&mut self, ctx: &egui::Context) {
        self.ctx = Some(ctx.clone());
    }

    fn input_hook(&mut self, input: &mut egui::RawInput) {
        if input.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. })) {
            self.owner = Some(input.viewport_id);
        }
        if let Some(st) = self.ctx.as_ref().and_then(state) {
            self.before.insert(input.viewport_id, st);
        }
    }

    fn on_end_pass(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let vp = ctx.viewport_id();
        let Some(before) = self.before.remove(&vp) else { return };
        let foreign = self.owner.is_some_and(|o| o != vp);
        let clicked = ui.input(|i| i.pointer.any_pressed());
        if foreign && !clicked && before.has_selection() && !has_selection(&ctx) {
            ctx.with_plugin(|p: &mut LabelSelectionState| *p = before);
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
        has_selection(ctx)
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

    /// A pop-out drawn as an immediate viewport runs its pass inside the main window's, after the
    /// main window drew the selected label: the selection must outlive both, frame after frame.
    #[test]
    fn a_selection_survives_a_window_nested_in_its_pass() {
        let ctx = egui::Context::default();
        install(&ctx);
        ctx.set_embed_viewports(false);
        let root = egui::ViewportId::ROOT;
        let inner = egui::ViewportId::from_hash_of("popout");
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut vp| {
            let _ = ctx.run_ui(raw(vp.ids.this, vec![]), |ui| (vp.viewport_ui_cb)(ui));
        });
        let copied = std::cell::RefCell::new(String::new());
        let pass = |events| {
            let out = ctx.run_ui(raw(root, events), |ui| {
                ui.label("pilot: form up now please");
                ui.ctx().show_viewport_immediate(inner, egui::ViewportBuilder::default(), |ui, _| {
                    ui.label("elsewhere");
                });
            });
            for c in out.platform_output.commands {
                if let egui::OutputCommand::CopyText(t) = c {
                    *copied.borrow_mut() = t;
                }
            }
        };
        let (a, b) = (egui::pos2(12.0, 10.0), egui::pos2(300.0, 10.0));
        pass(vec![egui::Event::PointerMoved(a)]);
        pass(vec![btn(a, true)]);
        pass(vec![egui::Event::PointerMoved(b)]);
        pass(vec![btn(b, false)]);
        for _ in 0..3 {
            pass(vec![]);
            assert!(has(&ctx), "the nested window's pass must not clear it");
        }
        pass(vec![egui::Event::Copy]);
        assert!(copied.borrow().contains("form up"), "{:?}", copied.borrow());
    }
}
