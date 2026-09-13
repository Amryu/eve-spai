# WEB-038 review cycle

**Status:** Fixed
**Branch:** `web/web-038-selected-range`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693 to 694, one new regression test |
| **Follow-ups** | none |

## The theme

The sheet now reads the theme out of the published snapshot, where it has been travelling all along
for the panes to draw with, and falls back to the config only before the first publish. So the theme
is no longer part of what the listener is configured with, and it comes out of the restart key.

`the_theme_sheet_follows_the_published_theme_without_a_restart` is the regression: it publishes a
meta with a different accent and asserts the *same* listener serves it. Without the fix the sheet
keeps the config's colour, which is exactly the state that made a restart necessary.

## The rebind

Kept as a second line of defence, because a port or LAN-binding change still has to replace the
listener and hits the same race. Twelve attempts, 80ms apart, so an orderly handover of a socket that
takes up to 500ms to close now succeeds instead of switching the feature off.

## The selection

`selected` is its own variable, set from the hash, and the tint and ring draw `hovered ?? selected`.
Hovering still wins while it lasts, which is what the app does and what the user asked for; before
this, selecting and hovering were the same variable and the pointer overwrote the choice.
