# UI-023 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-023-tab-drag-ghost`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 14.9 min across 1 round, 56 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 73/0 lines (added/removed), excluding the harness |
| **Harness code changed** | 162/2 lines |
| **Suite** | 441 to 443 passing |
| **Follow-ups** | corrected GAP-008's assessment of drag input |

## The change

Two helpers beside `jabber_tab_box`, driven from `jabber_tab_bar_ui`:

- `jabber_drag_ghost` paints a chip at the pointer: rounded rect in `window_fill`, 1px accent
  stroke, the tab label at body size, offset +14/+14 and clamped inside `content_rect()`. Sets
  `CursorIcon::Grabbing`. Painted through `ctx.layer_painter(LayerId::new(Order::Tooltip, ..))`, so
  it allocates nothing and senses nothing.
- `jabber_tab_lifted` veils the source tab with `panel_fill.gamma_multiply(0.7)` and outlines it in
  the accent colour, so the slot the tab came from reads as empty.

Position comes from `ui.ctx().pointer_interact_pos()`, the window-local pointer. **`TabDrag.at` is
never read**, so Wayland, where it is `None`, still gets the ghost. That was the first of the two
traps and it was avoided by construction rather than worked around.

## The scope cap did not bind

The user capped verification effort here, and I passed that on: time-box the synthesized drag, prefer
seeding state, and landing with no drag test was acceptable.

It turned out not to be needed. A synthesized drag worked on the first attempt.
`uitest_jabber_tab_drag_ghost_comes_and_goes_with_the_gesture` reads the tab's painted label rect out
of the shape list, presses at its centre, moves 8px to cross the drag threshold, moves into the
history, then releases. Mid-drag the label is painted twice and the second rect is within 40px of the
pointer; after release it is painted once. **Appearance and disappearance are both covered by a real
gesture.**

The cap was still the right call to make: it was cheap insurance against the case GAP-008 predicted,
and it cost nothing when the prediction turned out wrong.

## Correction to GAP-008

That gap lists jabber tab drag-and-drop as **large** effort and effectively untestable. That is now
too pessimistic. `harness.event` with coordinates **does** drive the tab drag. What is unavailable is
`get_by_label().click()` on painter-only tabs, and cross-window drops, which still need a viewport
rect kittest never fills. Recorded on the gap ticket.

The assertions read `harness.output().shapes` for `Shape::Text` galleys, recursing `Shape::Vec`,
because tabs and the ghost have no queryable node. That technique is new here and generalises to any
painter-only surface, which is most of GAP-003's map.

## The second trap, checked rather than assumed

An `Area` ghost would have tripped `uitest_layout` as an overlapping click target, which is exactly
why UI-020 moved the pin out of one. `uitest_jabber_tab_drag_paints_a_ghost_at_the_pointer` collects
every AccessKit bounding box from a dragging harness and a non-dragging one and asserts the two lists
are **equal**. The ghost adds no node and moves none. `uitest_layout` is green with the scene in
`all()`.

The permanent scene seeds `at: None`, the Wayland shape of `TabDrag`, so it doubles as proof the
ghost does not depend on the monitor-space field.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 441 passed, 2 ignored (+32) | **443 passed**, 2 ignored (+32) |
| with `--features fc-rescue` | 467 passed | **469 passed** |
| `cargo test --bin eve-spai uitest` | 32 passed | 34 passed |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Screenshot

`after/jabber_popout_tab_drag.png`: the cursor sits mid-history, and below-right of it is a small
dark chip with a cyan border reading `delve.imperium`, legible over the chat text. In the tab bar the
first tab is washed out, icon, label and close X all dimmed, with a cyan outline round the slot.
Second and third tabs untouched.

## Not covered, stated plainly

- Cross-window feedback: out of scope and unchanged. The ghost while dragging over a *different*
  window is not tested and cannot be with this harness.
- The `jabber_tab_lifted` veil is exercised by the scene and by eye, but no assertion pins its colour
  or outline. Only the ghost's presence and position are asserted.
