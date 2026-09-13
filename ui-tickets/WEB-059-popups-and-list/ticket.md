# WEB-059 &mdash; Layer popups open into a clipped box, warnings take a second line, the list stops short

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js`, `assets/dialogs.css` |
| **Reported by** | user |

## Symptoms

1. "Clicking the filter buttons on the map also does not open anything."
2. "Route lists: the warning makes it take an extra line. Make sure it fits in one line. It seems I
   also can't click the warning text to see more details in a dialog and there is no cursor highlight
   to indicate I could"
3. "The route list still doesn't use the full available height in the web view"

## Cause

1. The popups were children of the pane, which scrolls, and a scrolling box clips whatever hangs out
   of it. WEB-047 worked around that by positioning them as `fixed` and placing them on the next
   frame, which moved the problem rather than removing it: they are still inside the clipping box, and
   which layouts cut them off depends on the pane.

2. The warning was `flex: 1 1 100%`, which is a full row by definition.

3. The docked panel scrolled, so the hop list inside it sized to its own content and left the bottom
   of the dock empty.
