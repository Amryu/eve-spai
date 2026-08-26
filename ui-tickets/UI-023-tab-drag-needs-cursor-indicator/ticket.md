# UI-023 &mdash; Dragging a chat tab shows nothing at the cursor

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `jabber_tab_bar_ui`, `jabber_tab_box` |
| **Reported by** | user |

## Symptom

Dragging a chat tab gives no indication at the cursor that anything is being carried. The user
cannot tell a drag has begun.

## What already exists

- `TabDrag` (`app.rs:18264`) tracks the drag: the jid, the source window, a live monitor-space
  pointer position, and an `alive` flag the source sets each frame.
- `jabber_drop_highlight(win)` (`app.rs:3342`) outlines the **destination** window during a drag.
- The drag itself is driven at `app.rs:3563` from `drag_started_by` / `dragged_by` /
  `drag_stopped_by` on the tab's response.

So the state is all there. What is missing is feedback at the pointer.

## What to build

Something small following the cursor that names what is being dragged, at minimum the tab label.
Consider also dimming or outlining the source tab so its origin is obvious.

`TabDrag.at` is monitor-space and is `None` when the source cannot resolve a pointer position, which
the comment says happens on Wayland. Cross-window feedback therefore cannot rely on it. Feedback
inside the source window can use the local pointer position instead, and should, since that is the
case the user is reporting.

Paint it above everything: an `Area` at `Order::Tooltip` is the usual egui approach. Note UI-020
just moved the always-on-top pin out of an `Area` because it overlapped content, so if you use one
here, make sure it cannot be hit-tested or land in the AccessKit tree as a click target, or
`uitest_layout` will report it overlapping whatever it floats over. A drag ghost should not be
interactive.

## How to verify

The harness can drive this: `harness.event` with `PointerButton` press, then `PointerMoved`, is how
`drags_the_alert_window` in `scenes.rs` works, and `uitest_nav_rail_short_reaches_every_item` shows
the multi-pass pattern. Drag a tab a few px and assert the indicator exists and sits near the
pointer. Screenshot it mid-drag.
