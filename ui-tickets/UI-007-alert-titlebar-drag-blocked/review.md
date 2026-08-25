# UI-007 review cycle

**Status:** Fixed and verified
**Wave:** 4 (paired with UI-009 on `intel_row`, no region overlap)
**Branch:** `fix/ui-007-alert-titlebar-drag`

## The change

Both title-row labels in `build_alert_viewport_cb` become
`ui.add(egui::Label::new(...).selectable(false))`.

In egui 0.34 `Label::layout_in_ui` unconditionally ORs `Sense::click_and_drag()` into a label's
sense while `style.interaction.selectable_labels` is on, which is the default. That is what put a
click-and-drag hit target over the title text and the counter. `.selectable(false)` drops the
sense without touching sizing, and `Widget for Label` only branches between `LabelSelectionState`
and a plain `TextShape` when painting, so nothing moves.

## Correction to the ticket

My ticket said grabbing the title text hits the labels. That is not quite what happens, and the
agent's diagnosis is better:

At the exact centre of either label the drag rect still wins. egui's `hit_test_on_close` gets a
direct hit on both, and ties break toward the last-registered widget, which is the drag rect. The
failure lives in the 5px `interact_radius` band around and between the labels. There `hit_click` is
`None`, so egui takes its `(None, Some(hit_drag))` branch, finds the label as the nearest
click-sensing widget, sees the drag rect contains it, and applies the "small thing on a big
background, help the user hit it" rule. The drag goes to the label.

**I confirmed this rather than accepting it.** Stashing the `app.rs` half of the patch and running
the new test fails with:

```
dragging the gap between the title and the counter at [92.5 21.0] did not start a window drag
```

It fails at the gap, not at the title centre, which is exactly the corrected account. Restoring the
patch makes it pass.

## Ping window

Not affected, and not for the reason it looks. `build_ping_viewport_cb` has no custom title bar at
all, no drag rect and no `StartDrag`; it relies on the OS decoration. The only other `StartDrag` is
the main window title bar at `app.rs:9404`, out of scope. Nothing to ticket. This also bears on
UI-016, which asks whether the ping window should have chrome at all.

## Test added

`uitest_alert_titlebar_has_no_competing_grab_target`, with a `drags_the_alert_window` helper that
presses, drags right, and scans `harness.output().viewport_output` for
`ViewportCommand::StartDrag`. It probes four points along the bar: the title centre, the midpoint
of the gap (derived from the two AccessKit rects, not hardcoded), the counter centre, and an empty
stretch.

This is the first assertion in the suite that reads viewport commands, which is the technique
**GAP-007** identified as the cheap way to cover the 12 `send_viewport_cmd` sites. It works, so that
gap is now partly closed and the pattern is available to the rest.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 412 passed, 2 ignored (+32) | **413 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 13 passed | 14 passed |
| Test fails without the fix | n/a | confirmed, at the gap |

## Screenshots

- `before/alert_window_typical.debug.png`: a blue `drag` outline spans the bar with two magenta
  `click+drag` boxes sitting on top of it, one tight around "Intel alerts" at x~8..89 and one around
  the counter at x~96..109.
- `after/alert_window_typical.debug.png`: both magenta boxes are gone. One blue `drag` outline runs
  from the grip dots to the button cluster, and the text draws in the same places.

On the plain PNGs the agent reported byte-identical renders. Mine differed, so I measured rather
than trusting either: **45 pixels inside a 7x9 box at (78,67)**, which is a digit in the card's age
text, not the title bar at y=7..35. That is the known fixture clock drift, since the alert window
computes ages from the wall clock and `fixtures::now()` is stamped per run. The title bar is
visually inert, as claimed.

## Warning I introduced

The agent flagged a second `unused_mut`, at `app/src/uitest/scenes.rs:414`, in the
`uitest_battles_view_settles_without_a_worker` test I added while landing UI-005. I had reported
that check as clean and it was not. Fixed on main in `c2ea1ef` before this branch, so
`cargo check --workspace --all-targets --all-features` is back to the single pre-existing warning at
`app/src/intel.rs:5605`.

## Rejected

- `ui.style_mut().interaction.selectable_labels = false` for the row: wider blast radius, would
  silently cover any widget added to the row later.
- Moving or shrinking the drag rect: changes which pixels are grabbable, and the ticket asked for
  the layout to stay put.
