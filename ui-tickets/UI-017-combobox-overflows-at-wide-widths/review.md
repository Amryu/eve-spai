# UI-017 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-017-combobox-wrap`
**Found by:** UI-012's width sweep

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 9.1 min across 1 round, 47 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 66/26 lines (added/removed), excluding the harness |
| **Harness code changed** | 24/0 lines |
| **Suite** | 445 to 446 passing |
| **Follow-ups** | none |

## The mechanism, which is the valuable part

A wrapping toolbar can never break *before* a `ComboBox`:

1. `ComboBox::show_ui` opens with `ui.horizontal(..)` (`egui/src/containers/combo_box.rs:229`).
2. `horizontal_with_main_wrap_dyn` sets that row's desired size to
   `available_size_before_wrap().x`, which is exactly whatever is left of the current row.
3. `Layout::next_frame` (`layout.rs:517`) breaks a wrapping row only when
   `available_size.x < child_size.x`. The ComboBox asks for **precisely** `available_size.x`, so
   that comparison is never true.
4. Inside, the box is now in a non-wrapping layout, so `wrap_mode()` is `Extend`, `wrap_width` is
   `INFINITY`, and the selected text lays out at full width. It then paints
   `max(galley.x + icon_spacing + icon_width + 2*button_padding.x, combo_width)`, which for
   "Balanced" is 109.7px, into a row edge with ~16px left.

The reservation was the leftover space; the paint was the intrinsic width. They were never the same
number.

## The fix

`toolbar_combo` measures the selected text with egui's own
`into_galley(Extend, INFINITY, TextStyle::Button)`, reproduces egui's own width formula, and wraps
the ComboBox in `allocate_ui_with_layout(vec2(w, interact_size.y), ..)`. The wrap decision then runs
against the real width, and the child's `max_rect` equals what the box paints, so nothing is clipped
or shifted.

It holds at every width because it depends on no width: it is the same arithmetic egui runs one call
later, evaluated one call earlier where it can still influence the wrap. It also covers longer
selected text and a different font, not just the word "Balanced".

Applied to all three ComboBoxes in the two battles toolbars (`work_throttle`,
`battle_roster_sort`, `br_manage_as`). All three had the identical defect; only the first happened
to be reachable headlessly.

## The sweep found more than the ticket knew

`uitest_battles_toolbar_stays_inside_the_window` sweeps 720 to 1600 in 40px steps. Before the fix it
failed at **800, 1440, 1480 and 1520**. The ticket knew only about 1440 and 1520.

800 is worth noting: it reported `content is 816px wide in a 800px window` with no escaped node,
because the escape was under the per-node tolerance but not under the used-rect one. Two of the
checker's rules catching different halves of the same bug.

A one-off fine sweep of 700 to 2560 in **3px steps, 621 widths**, was run and then removed: clean
everywhere. That is the evidence this is not a two-width patch.

**Teeth confirmed independently.** With the tests in and the fix reverted, the sweep fails with the
ticket's exact rect `[[1417.5 87.5] - [1527.2 114.5]]` at 1440 and again at 1480.

UI-012's own divider sweep still passes, so the new reservation did not push a divider to a row edge.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 445 passed, 3 ignored (+32) | **446 passed**, 3 ignored (+32) |
| with `--features fc-rescue` | 471 passed | 472 passed |
| `cargo test --bin eve-spai uitest` | 36 passed | 37 passed |
| Scenes | 28 | 29 |

`cargo check --workspace --all-targets --all-features`: the pre-existing `unused_mut` at
`app/src/intel.rs:5605`. The agent also noted a second pre-existing warning, `fn render is never
used` at `app/src/sound.rs:402`, which appears only in the default-feature test build because it is
`fc-rescue`-gated. Not touched by this diff.

## Screenshots

- `after/view_battles_wide.png` (1440x800, new permanent scene): row 1 ends at "Build from kill"
  with clear margin to the right edge, and "Balanced" has moved down to row 2 beside "Enabled",
  fully inside the panel with its divider painted. The GAUGE glyph renders as a gauge, not tofu.
- `after/view_battles_wide.debug.png`: the ComboBox's interactive rect exactly bounds the drawn
  frame, no overlap.
- `view_battles.png` (1280) and `view_battles_narrow.png` (720) are unchanged.
- `view_battle_detail_narrow.png`: that combo is narrower than `combo_width`, so it still uses the
  100px minimum, as before.

## Note

This is an egui behaviour rather than an app bug, so any future wrapping toolbar with a ComboBox in
it will hit the same thing. `toolbar_combo` is the place to reach for.
