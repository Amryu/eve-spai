# UI-020 &mdash; Always-on-top pin floats over popout content

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | `ontop_pin` + `jabber_tab_bar_ui` |
| **Reported by** | user |
| **Blocked on** | GAP-004 for visual verification |

## Symptom

In a popped-out chat window the always-on-top pin overlaps the window content. It is drawn as a
floating `egui::Area` anchored `RIGHT_TOP` at `Order::Foreground` (`app.rs:21305-21307`), so it sits
on top of whatever the central panel renders underneath, obscuring message text and tab-bar items at
the top right.

## Requirements, from the user

1. It must never interfere with window content.
2. It must not reserve much space of its own.

## Cause

`ontop_pin` uses an overlay `Area` rather than participating in layout, which is exactly why it
overlaps: an `Area` is positioned independently of the panels beneath it.

## Suggested direction, not binding

`jabber_window_body` (`app.rs:3324`) already opens a top `Panel` for the tab bar. Right-aligning the
pin inside that existing row satisfies both requirements at once: it is in the layout so nothing can
sit under it, and the row already exists so it costs no extra height. Weigh alternatives before
committing.

`ontop_pin` is also called for other viewports; grep every caller before changing its signature, and
either keep them working or say what you changed.

## How to verify

Needs a jabber popout scene, which does not exist yet (GAP-004). Render the popout with enough
messages that content reaches the top-right corner, and confirm nothing is drawn over the pin and
the pin is drawn over nothing. `uitest_layout` must show no overlapping click targets and no
overlapping text.
