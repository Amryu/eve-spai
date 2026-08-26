# UI-011 review cycle

**Status:** Fixed and verified
**Wave:** 5 (paired with UI-012 on `battles_view`, no region overlap)
**Branch:** `fix/ui-011-reporter-footer`


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 7.7 min across 1 round, 54 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 12/10 lines (added/removed), excluding the harness |
| **Harness code changed** | 124/1 lines |
| **Suite** | 414 to 415 passing |
| **Follow-ups** | none |

## The change

The badge closure is handed to `horizontal_wrapped` on its own, and the reporter label is emitted
after it into the frame's own vertical layout:

```rust
ui.horizontal_wrapped(render);
if show_reporter && !is_zkill {
    ui.add_space(if compact { 1.0 } else { 3.0 });
    ui.add(egui::Label::new(egui::RichText::new(..).weak()).wrap());
}
```

That is the cause fix rather than a spacing tweak: the footer is no longer a participant in the
chip flow, so it cannot begin mid-row after the last badge, and it wraps at the vertical layout's
pitch instead of the wrapped flow's tight line pitch.

The leading `·` is gone. It only ever existed to separate the footer from the chip preceding it
inline; on its own row it rendered as an orphan bullet, which is visible in the 320px before shot
trailing the CAP TACKLED chip. The interior `reporter · channel` separator, the weak colour and the
font size are unchanged.

`let mut render` lost its `mut`, since it is now moved into the call and used once. Without that,
`cargo check` gains a new `unused_mut`.

## The `show_reporter == false` case

The guard now wraps the `add_space` as well as the label, so nothing at all is emitted when the
footer is off and no trailing gap is left behind. This matters because the alert overlay passes
`show_reporter: false`. Verified two ways: `alert_window_torture.png` is unchanged apart from
wall-clock age digits, and the new test measures card height with the footer on and off and requires
the off case to be at least 10px shorter.

## The ticket's screenshots were stale

The existing `intel_row_torture` scenes are 520px tall and the card now runs to roughly 940px, so
the footer is cropped ~350px below the frame and **the bug region was not visible in any rendered
PNG**. That is a side effect of my own earlier fixture fix: once the twelve torture pilots started
resolving, the card grew by eleven chip rows and pushed the footer out of shot.

The agent said so and added `intel_row_torture_full` (520x1000) and `intel_row_torture_narrow_full`
(320x1400) rather than quietly rendering something that looked close enough. That is the second
ticket this session whose `before/` evidence turned out not to show the bug, after UI-009.

Lesson for the scene set: a scene that crops its subject is worse than no scene, because it looks
like coverage. Recorded in CLAUDE.md.

## Test added

`uitest_intel_row_reporter_is_a_footer` asserts the footer label's top is at or below every chip's
bottom, that its left edge is not indented past the leftmost chip, and that hiding the reporter
shortens the card by more than 10px. **Confirmed it fails on the unfixed code** by stashing the
`app.rs` hunk.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 414 passed, 2 ignored (+32) | **415 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 15 passed | 16 passed |
| Screenshot scenes | 54 PNGs | 56 PNGs |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Screenshots

- **320px** (`after/intel_row_torture_narrow_full.png`): before, a lone `·` hangs off the end of the
  CAP TACKLED row, then two cramped lines butt straight against the chips, three ragged pieces.
  After, the flag chips end cleanly, then a visible gap, then two tidy footer lines starting at the
  card's left margin.
- **520px** (`after/intel_row_torture_full.png`): before, the reporter starts on the same line as
  the CAP TACKLED chip and wraps back to the margin with almost no leading. After, the badge row
  ends at CAP TACKLED, then a gap, then two left-aligned footer lines.
- **`after/intel_row_typical.png`**: the footer moves from beside the pilot chip to its own row
  underneath, left-aligned with them.
- **`after/alert_window_torture.png`**: unchanged. No footer on that path, no new blank rows, card
  spacing identical.

## Rejected

- An extra `ui.vertical(..)` wrapper: `Frame::show` already provides a vertical `Ui`.
- A `ui.separator()` above the footer: the alert overlay stacks these cards tightly and a rule per
  card turns the feed into ladder stripes.
- Right-aligning the footer: breaks alignment with the chip column down a stack, and hurts at 320px
  where the text needs two lines.
- Shrinking the footer text: ruled out by project convention.
- Editing the existing torture scenes' heights instead of adding new ones: would have invalidated
  comparison against the ticket's `before/` images.
