# Working with the harness

Commands are in `CLAUDE.md`. This file is what the harness does not tell you.

## Adding a scene

Append to `scenes::all()`. Then run the census:

```bash
cargo test --bin eve-spai uitest_census -- --ignored --nocapture
```

A scene sitting near the ~12-target chrome baseline is not being inspected in any meaningful sense,
whatever its verdict says. It rendered the frame and nothing in it.

Size the scene to its whole subject. Two tickets in the first round shipped `before/` screenshots
that did not contain their own bug: one scene never rendered the chip at all, the other put the
footer 350px below the frame. Both were caught by the agent doing the fix, not by the review.

Expect a new scene to fail `uitest_layout` immediately. Three of the first scenes added did, each on
a pre-existing bug nobody had filed. That is the harness working.

## Dialogs

`App::ui` calls `root_chrome`, `root_central`, `root_dialogs`; the harness calls the same three.
`scenes::dialog_scene(name, size, |a| ...)` handles viewport routing. Make the gate field
`pub(crate)`, set it plus whatever the body reads, push into `all()`, add the landmark string to
`uitest_dialog_scenes_render_their_dialog`.

A gate field is not always the whole story: `coalitions_window` also needs `coal_edit`, which the
button that opens it fills in.

Size it to the `[w, h]` the dialog passes to `dialog_viewport`. Plain `egui::Window` and `Modal`
float, so give those room around them instead.

## What the checker cannot see

`checks.rs` catches overlapping click targets, overlapping text, horizontally escaped widgets,
zero-area hit rects, and content wider than its window.

It is blind to painted decoration, because separators, canvas art and custom-drawn overlays emit no
AccessKit node. The screenshots stay the primary signal; the assertions are the regression gate. Do
not conclude a scene is clean because `uitest_layout` passed.

It also never compares a hit target against text (GAP-010), which is how a pin button sitting on top
of a dialog's own content stayed hidden through a review that specifically looked for it.

## Fixture traps

- `intel_row` skips any pilot missing from `resolved_pilots`, so unresolved fixture names render
  nothing at all. A fixture can look full and produce an empty row.
- Headless disables the workers that populate views, so async-populated views show permanent loading
  states. That is correct behaviour, not a bug to file.
- `SpaiApp::build(ctx, headless: true)` refuses to open a store unless `EVE_SPAI_DATA_DIR` is set.
  Never point it at the real profile.

## egui behaviours found the hard way

Each of these cost a ticket to diagnose.

- `horizontal_wrapped` floors row height at `interact_size.y`. A row of 15px text allocates 26px.
- Setting `interact_size.y` inside a closure does nothing. egui reads it off the parent `Ui`.
- `ComboBox::show_ui` asks for exactly `available_size_before_wrap().x`, so a wrapping row never
  breaks before it and the combo runs off the edge instead.
- `available_width()` returns the whole row in a wrapping layout, not the remaining space.
- `ScrollArea` inflates its clip rect by `clip_rect_margin`, so content appears to escape by a few px
  when it has not.
- `RichText::strong()` is colour-identical to body text when `override_text_color` is set. Size is
  then the only lever, and using it is usually the wrong call (see UI-003).
- Hyperlinks report `Role::Label`, not `Role::Link`.
- `Tooltip::for_enabled` gates on `response.enabled()`, so `on_hover_text` on a disabled widget never
  fires. A tooltip explaining why a control is disabled is unreachable by construction.
- egui has no built-in virtualization. Variable-height rows need `ScrollArea::show_viewport` plus a
  per-row height cache.

## Interactions the harness will not reach cheaply

Drag-and-drop is the known one. Seed the resulting state and render it rather than simulating the
input. If that fights back too, land the fix and record what is uncovered. This cap is the user's
call, stated directly: do not overengineer a way to test dragging.
