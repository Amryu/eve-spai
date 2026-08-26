# UI-003 review cycle

**Status:** Fixed and verified
**Wave:** 1 (paired with UI-001 on `nav.rs`, no region overlap)
**Worktree:** `wt/ui-003`, seeded from the main tree's working state


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 5.1 min across 1 round, 33 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 8/1 lines (added/removed), excluding the harness |
| **Harness code changed** | 0/0 lines |
| **Suite** | 409 to 409 passing |
| **Follow-ups** | none |

## The change

`app/src/app.rs`, `render_ping`, the `Ping::Fleet` arm. 8 insertions, 1 deletion, nothing else touched.

```rust
if !description.is_empty() {
    // The call text has to outrank the metadata labels above it, and the theme
    // paints `strong` in the same color as body text, so size is the only lever.
    ui.scope(|ui| {
        if let Some(f) = ui.style_mut().text_styles.get_mut(&egui::TextStyle::Body) {
            f.size += 1.5;
        }
        render_ping_body(ui, description, false);
    });
}
```

## Review

The obvious fix was flipping `true` to `false`, and that alone would have been wrong. It removes
the inversion but leaves the fleet call tied with its own metadata labels, so the urgent line still
would not read as the card's primary content.

The agent's claim that weight and colour are both unavailable checked out:

- `theme.rs:120` sets `override_text_color = Some(fg)`, so every plain text run resolves to the
  same colour.
- `theme.rs:148` sets `widgets.active.fg_stroke` to that same `fg`, and egui's
  `strong_text_color()` returns `widgets.active.text_color()` (`egui/src/style.rs:1149`).

So `RichText::strong()` is colour-identical to body text in every one of this app's presets, and
egui has no font-weight concept to fall back on. That leaves size, which is what the comment
records. The comment justifies a non-obvious constraint rather than restating the code, which
matches the project convention.

`ui.scope` restores the style on exit, so the bump is contained to the body and does not leak into
the footer or the next card.

## What was rejected

- Flipping the boolean alone: fixes the inversion, does not fix the ranking.
- `RichText::strong()`: a genuine no-op here, so it would have looked like a fix in the diff while
  changing nothing on screen.
- Weakening the FC / Formup / Doctrine labels to raise the body by contrast: those are the details
  a pilot acts on, and dimming them trades one legibility bug for another.
- Changing `render_ping_body`'s third parameter to an emphasis enum: cleaner API, but it forces
  edits into the test region at app.rs:23517 and outside the ticket's scope.

The `weak` parameter now only ever receives `false` from production code. Both remaining `true`
call sites are in `ping_link_tests::ping_bodies_render_without_panicking`, a smoke test that pushes
awkward bodies through both branches to prove neither panics on a byte-index slice. Left in place
deliberately; removing the parameter would have meant editing that test for no user-visible gain.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 409 passed, 2 ignored | 409 passed, 2 ignored |
| `cargo test --bin eve-spai uitest` | 10 passed | 10 passed |
| Layout assertions | clean | clean |
| `ping_fleet` smallest hit target | 17px | 17px (unchanged) |

`cargo check --workspace --all-targets --all-features` reports only the pre-existing unused-`mut`
warning at `app/src/intel.rs:5605`. No new warnings.

## Screenshots

`before/ping_window_mixed.png` against `after/ping_window_mixed.png` is the clearest pair, because
it stacks both ping kinds.

- **Before:** the fleet call body renders in muted grey, dimmer than the "FC:" and "Formup:" rows
  directly above it, and dimmer than the routine skill-queue reminder in the card below. The urgent
  line is the faintest content on screen apart from the footer.
- **After:** the same sentence renders at full foreground and one step larger than the metadata
  rows, so it is the first thing the eye lands on inside the card. The plain ping below is
  unchanged and now reads as clearly secondary. Card hierarchy top to bottom is intact: header row,
  body, metadata, weak footer.

## Residual risk

In the fixed 512px `ping_fleet` scene the longer body line now ends about 3px from the frame edge.
It does not overflow, and the layout checks accept it. The real window is resizable and wraps
normally, but a longer single-line description in a narrow window will now wrap one word earlier
than before. Acceptable; noted in case a future ticket reports it.

The ping card footer is still `.small()`, which is UI-008's scope, not this ticket's.
