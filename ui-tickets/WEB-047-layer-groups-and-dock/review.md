# WEB-047 review cycle

**Status:** Delivered
**Branch:** `feat/web-047-layer-groups`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 701, unchanged |
| **Follow-ups** | the in-app context menu and save/load routes are still open |

## Four words instead of eleven buttons

Sov, Activity, Travel, Marks. Each opens the handful of switches behind it, and the button lights when
anything in its group is drawing, so the row says what is on without being opened. Two marks, because
"this group is doing something" and "this group's popup is showing" are different facts.

The popup closes on exactly the three things asked for: elsewhere, another group's button, the same
button again. Using a switch inside it does not close it, which is the point: a layer panel is a thing
you set two or three of at once.

Placed as fixed and measured, not anchored: the pane scrolls, and a scrolling box clips what hangs out
of it, so anchored inside the pane half the popup was cut off at the edge. Measured on the next frame,
because opened from the deep link the toolbar has not been laid out yet and the button's rectangle is
still zero.

## The dock

The canvas and the route panel share a row, so docking takes space from the map instead of covering
it, which is the difference between docking and floating. A wide pane gives the dock the right-hand
edge and a tall one the bottom, decided from the pane's own shape.

The panel may be created before the map pane exists, since a deep link opens it during load, so the
map's own "I have a canvas" event is what moves it in. Docked, the hop list is the part that scrolls.

## Avoidance, finished

An avoid button on every hop the route merely passes through, and a line saying how many systems are
being planned around with a way to clear them. Not on the systems the user named: endpoints are exempt
from avoidance anyway, so the button would be there and do nothing.

"Stop avoiding always" appears only where there is something to stop, which needs the app's lists;
they travel in the snapshot for that and for the marks.

While a route is being planned, avoided systems are crossed out on the map. A cross rather than a
ring, because a ring is what this map uses for "look here" and this is the opposite. Only in routing
mode: off it they are just systems, and someone with a long list would have most of the map marked.

## The waypoint highlight

The whole row is tinted. An outline on the chip read as a focus ring, which is a different thing
entirely.

## Verified

`after/layer-groups.png`, `after/route-docked-right.png`.
