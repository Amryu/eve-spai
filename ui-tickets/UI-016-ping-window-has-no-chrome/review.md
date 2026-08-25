# UI-016 review cycle

**Status:** Closed, not a defect
**Resolution:** False positive from the audit. No code change.

## What the ticket claimed

The alert window has a title bar, a four-button cluster and a resize grip; the ping window has
none of these, so it cannot be moved, closed or resized. Raised as a product decision.

## What is actually true

The two windows differ because one is undecorated and the other is not, which is deliberate.

| | Alert window | Ping window |
|---|---|---|
| `with_decorations` | `false` (`app.rs:19815`) | never called, so the default applies |
| OS frame | none | window manager title bar and close button |
| Custom chrome | title bar, buttons, resize grip, drag rect | none needed |
| `with_resizable` | `true` | `true` |

`grep` for `with_decorations` and `ViewportCommand::Decorations` across `app.rs` and `overlay.rs`
finds no later override for the ping viewport. The alert window builds its own chrome precisely
because it has no OS frame to rely on. The ping window inherits a real title bar, a real close
button and OS resize handles.

So the window can already be moved, closed and resized. Nothing to fix, and nothing for the user
to decide.

## Why the audit got it wrong

The harness renders **viewport contents, not the OS frame**. A decorated window screenshots exactly
like an undecorated one, because the window manager's title bar is never part of the render. Both
`ping_window_fleet.png` and `alert_window_typical.png` show only the client area, and the alert
window's chrome appears in its render solely because that chrome is drawn by the app.

This is a real limitation, not a one-off mistake, and it will produce the same false positive for
any future ticket about window furniture. Recorded on GAP-007, which already covers what the
harness cannot see about real window behaviour.

## Related

UI-007 touched the same ground from the other side: the alert window's drag rect exists only
because that window is undecorated. That fix stands, and it does not apply here since the ping
window has no custom drag rect and needs none.
