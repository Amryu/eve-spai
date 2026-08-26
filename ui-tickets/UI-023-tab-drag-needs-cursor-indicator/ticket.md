# UI-023 &mdash; Dragging a chat tab shows nothing at the cursor

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
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

**Do not overengineer this.** The user has said they would rather give feedback on the drag
themselves than have effort sunk into simulating it, and GAP-008 already records tab drag-and-drop
as the hard case: the tab is a painter-only `ui.interact` rect with no queryable AccessKit node.

In order of preference:

1. Seed `jabber_tab_drag` directly and render one frame. The popout scenes already seed private
   `SpaiApp` fields, so this gets a mid-drag screenshot for almost nothing.
2. Time-box synthesizing real pointer events. If a press-move-release does not produce a live drag
   quickly, stop.
3. Landing with no drag-specific test is acceptable, as long as the report says so plainly.

The rest of the bar is unchanged: full suite green including `--features fc-rescue`, `uitest_layout`
green, no new warnings, and look at whatever is rendered.
