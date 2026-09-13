# UI-049 &mdash; The tray and taskbar say only "something is unread", not how much

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `tray.rs`, `badge.rs`, `SpaiApp::update` |
| **Reported by** | user, with UI-048 |

## Symptom

"The tray icon and the task bar icon should update based on the amount of new messages, similar to
discord. (1-99 and then 99+)"

The tray drew a plain red dot for "anything at all is unread". The taskbar icon never changed.

## Cause

Unread was a boolean set, so a dot was all there was to draw. There is also no text rasteriser
reachable from the tray thread: the app's fonts live inside egui, and the tray icon is painted on a
background thread with no context.

## Deliverable

A count on both icons, 1 to 99 then `99+`, matching the sidebar badges from UI-048. Muted
conversations do not contribute: a conversation the user has silenced does not get to put a number on
their taskbar.

## How to verify

Unit tests on the painter: a count paints, zero does not, a wider count takes more room, and nothing
lands outside the icon. The icons themselves are verified by eye. A fix would be WRONG if it repainted
the window icon every frame: `ViewportCommand::Icon` hands the window manager a fresh image.
