# WEB-045 review cycle

**Status:** Delivered
**Branch:** `feat/web-045-titan-start`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered; interpretation of 1 stated below |
| **Suite** | 700, unchanged |
| **Follow-ups** | the desktop's warning is not clickable yet |

## Which end the titan is at

Checked, which is the default, the titan is in the system the route starts from: one jump out as far
as range allows, then gates. That is what the calculation already did; the checkbox makes it an
assumption the user can see and change rather than one baked in.

Cleared, the titan is waiting at the far end: gates out to the best system it can reach, then bridged
in. Implemented by running the same search backwards and turning the result around, so there is one
implementation of "one jump, gates for the rest" rather than two that can drift.

Reversing a route is not just reversing the list: a hop's kind, distance and fuel describe the edge
*into* it, so they shift one place as well. Without that the jump would be reported on the wrong
system.

## Bridges

The picked route arcs a bridge, like every other layer on this map. A bridge is a bridge whether or
not a route happens to be using it, and a straight line said it was a gate.

## The intel behind a warning

"Danger intel 4m" is a summary of something somebody wrote, and the words are the part worth reading.
The warning is now a button when there is intel behind it, opening a modal with the cards for that
system, rendered by the same `card` the feed uses.

A modal, not another floating window: it is opened from one, would land on top of the thing that
named it, and is read and dismissed rather than kept beside the map.

**Not done:** the desktop's warning line is still a label. The request's "not a hovering window"
describes the browser's two kinds of window, so that is where this landed; say the word and the
desktop gets the same.
