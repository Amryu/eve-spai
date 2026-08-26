# GAP-001 review cycle

**Status:** Mechanism proved, 8 of 28 covered. Remaining 20 are mechanical.
**Branch:** `harness/gap-001-dialog-coverage`

## Resolution

| | |
|---|---|
| **Outcome** | Opened, first batch covered |
| **Agent time** | 18.1 min across 1 round, 93 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 54/47 lines (added/removed), the `root_dialogs` extraction plus `pub(crate)` |
| **Harness code changed** | 171/0 lines |
| **Suite** | 472 to 473 passing |
| **Follow-ups** | UI-033, GAP-010 |

## The embedded-viewport problem, solved in the harness

The ticket's analysis was right, and there was a second failure it missed.

`dialog_viewport_ext`'s callback takes `&mut Ui`, kittest leaves `embed_viewports` at `true`, so
`show_viewport_immediate` wraps the body in a `Window`. The body then opens a `CentralPanel` on
what derefs to the **root** `Context`, so the dialog paints full-screen with an empty 210x53
title-bar stub floating on its corner.

The part the ticket missed: `root_dialogs` also calls `alert_window` and `fleet_ping_window_ui`,
which call `show_viewport_deferred` unconditionally. Embedded, those became two more `Window`s each
opening another `CentralPanel` on the same context, painting egui's red "Double use of widget ID"
boxes over the dialog. **Any naive dialog scene gets those.**

Both fixed with two lines in the harness, no app change:

```rust
ctx.set_embed_viewports(false);
egui::Context::set_immediate_viewport_renderer(|ctx, mut vp| {
    let mut ui = detached_ui(ctx);
    (vp.viewport_ui_cb)(&mut ui);
});
```

`set_embed_viewports(false)` sends the deferred overlays to the backend branch, and kittest has no
backend, so they never paint. The immediate renderer stops the fallback so the dialog lands on the
root cleanly.

## Coverage

`root_dialogs(&mut self, ctx, jframe)` extracted from `App::ui` statement-for-statement, following
the `root_chrome`/`root_central` precedent. 8 gate fields plus `coal_edit` opened to `pub(crate)`.

Eight scenes, picked for mechanism first: five through `dialog_viewport` (the 22-dialog family, so
proving one proves the route), one plain `Window`, one `Modal`, one over `pickers::body`.

| scene | hit targets |
|---|---|
| `dialog_battle_filter` | 22 |
| `dialog_routes` | 20 |
| `dialog_severity` | 16 |
| `dialog_coalitions` | 15 |
| `dialog_filter_picker` | 9 |
| `dialog_intel_channels` | 8 |
| `dialog_jump_bridges` | 6 |
| `dialog_verdict_explainer` | 1 |

The 1 is honest rather than degenerate: that modal is three paragraphs and a "Got it" button, and its
labels do go through the text pass. All eight PNGs show real dialogs with real content, which I
checked.

`uitest_dialog_scenes_render_their_dialog` asserts each carries a known string, because
`uitest_layout` stays green on a blank scene. That is the right guard for this kind of coverage.

## Pre-existing bug found, not fixed: UI-033

**The always-on-top pin overlays the top-right of every `dialog_viewport` dialog.** Measured
overlaps in three, and in `dialog_jump_bridges` the pin sits **on a clickable hyperlink**. I
confirmed it in the PNG.

This corrects UI-020's review, which I signed off on. That review's caller table said
`dialog_viewport_ext` was "unchanged, and it is the shared body of ~18 dialogs. None has a tab row
to host a pin", and I accepted the implication that leaving the floating `Area` there was harmless.
It is not: the pin reserves no space, so every dialog body lays out underneath it. The reason those
dialogs were never seen to be broken is that **none of them had a scene until now**.

## Second finding: the checker cannot see this class at all, GAP-010

`uitest_layout` stays green on the overlap above. The click-target pass compares hit target against
hit target; the text pass compares `Label` against `Label`. **Nothing compares a hit target against
text.** The hyperlink also reports `Label` rather than `Link`, so even the click-target pass cannot
see that particular collision.

The agent left the checker alone, correctly. Fixing it would immediately fail these new scenes on
UI-033's bug, which is the right sequencing: land the coverage, fix the bug, then tighten the
checker.

## The pattern, for the remaining 20

1. `pub(crate)` the gate field.
2. `dialog_scene(name, size, |a| { ..seed.. })`, sized to the `[w, h]` the dialog passes
   `dialog_viewport`; `Window` and `Modal` dialogs get room around them since they float.
3. Push into `all()` and add a landmark string to `uitest_dialog_scenes_render_their_dialog`.

One trap, documented: a gate field is sometimes not the whole story. `coalitions_window` iterates
`coal_edit`, an edit buffer the settings button fills, so seeding only the gate rendered an empty
list. Written up in `app/src/uitest/mod.rs`.

## No pre-existing render changed

Verified with unusual care, and worth repeating as a method. Of 104 PNGs, 37 were byte-identical and
67 differed. To separate the change from clock noise the agent rendered the **unmodified baseline
twice** and diffed baseline against baseline: 65 of those 67 differ run to run with identical code.
The 4 remaining are sub-pixel or one-digit ("34s ago" against "35s ago", a 1/255 channel delta). No
geometry difference anywhere, consistent with a diff of `pub(crate)` and a statement move.
