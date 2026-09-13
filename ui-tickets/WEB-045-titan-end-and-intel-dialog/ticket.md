# WEB-045 &mdash; Which end the titan is at, bridges drawn straight, and an unreadable warning

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/map.js`, `assets/dialogs.*`, `app.rs` |
| **Reported by** | user |

## Symptoms

1. "'Titan route' should default to the titan being in the starting system. So we get an initial jump
   and that's it (make it a checkbox)"
2. "The displayed route draws a straight line between jump bridges instead of changing the draw style
   of it"
3. "The intel warning should be clickable, opening a dialog (not a hovering window) showing the
   relevant intel cards."

## Cause

1. The assumption was hardcoded and unstated: the search only ever ran from the start.

2. The picked-route drawing arced a capital jump and drew a bridge as a straight line, so on the one
   layer where it matters a bridge looked like a gate.

3. The warning was a summary of something somebody actually wrote, with no way to reach the words.
