# UI-055 review cycle

**Status:** Delivered
**Branch:** `feat/ui-055-route-sidebar`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 702, unchanged |
| **Follow-ups** | saving and loading routes is still open |

## The panel

Three kinds at the top, then the anchors, then the route. The ship is one collapsed header away and
only exists for the kind of route that has a ship, which is what made the old panel feel like a form.

It draws `map_route_opts`, the same route the map draws, so the sidebar and the map cannot disagree.
The jump route no longer hands itself off to a different panel: there is one route panel now, and it
covers gates, jumps and the titan hop.

Waypoints get their own tinted ground, so the route reads as the legs it was built from. Avoidance is
a collapsed header with the count, opening to the systems themselves with a remove on each, because a
count on its own is not something anyone can check.

## The intel

A warning line with intel behind it is a button, and it opens that system's reports in a window. A
line that only says "3 kills this hour" is not a button, because it has nothing to open.

## What avoidance cost

A leg is planned a second time without the avoid list, and if the answer differs the route says so:
"2 jumps longer, avoiding systems". Worth the extra search. "Why is this going the long way round" is
otherwise unanswerable from the list, and most routes avoid nothing, so most of the time the second
search finds the same path and nothing is shown.

## The small ones

The avoid button is ordered onto the name's line rather than after the two full-width rows that were
pushing it down. The system badge has a baseline width, so a column of null-sec names lines up instead
of jittering with the character widths; longer names still grow. Docked, the hop list takes whatever
height is left rather than stopping at a guess.
