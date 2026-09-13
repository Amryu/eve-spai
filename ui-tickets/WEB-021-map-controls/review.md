# WEB-021 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-021-map-ui`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 680 to 681 |
| **Follow-ups** | none |

## The layer panel

Open by default wherever there is room, laid out in the flow above the map rather than floating over
it, and its own button hidden because a panel that is always there does not need one. A click outside
closes it only where it is an overlay: on a wide pane, closing the layers because the user clicked the
map would be the map taking its own controls away.

"Room" is measured on **the pane**, not the viewport. The map can be one of four columns on a wide
desktop, and a viewport query would call that pane roomy when it is narrower than a phone.

Fit is gone. Double-click already frames a region and there was nothing left for the button to do
that a gesture did not.

## Upgrade marks

Sized from the dot radius, which is clamped at 3px, so a mark was under three pixels at every zoom.
The filter was working; what it drew was invisible. They are sized in screen pixels now and drawn
only once the map is zoomed in enough for them to have somewhere to sit.

## Jump range

This one needed data that was not being sent. The drawn coordinates are a projection quantised into a
0..4096 box; asking them for a distance in light years gives an answer distorted by however much the
projection distorts. So the geometry carries `pos3`, the real positions in hundredths of a light year,
parallel to the nodes: three small integers each, because at that precision the galaxy fits in about a
million units and the error is far below what a range band cares about.

Bands are drawn with `units_per_ly`, which is exact for a geographic layout and the same
approximation the app already makes for any other. The in-range tinting uses the real positions, so
that part is exact regardless.

## The system window

A modal takes the whole page to show one system, dims what is behind it, and blocks the map that was
the reason it was opened. It is a floating window now: draggable by pointer so a phone can move it,
clamped so it cannot be parked off screen, dismissed by Escape or its own button, and **not** by
clicking the map, because clicking the map is how the next one gets opened. On a phone it sits along
the bottom where a thumb reaches.

Clicking a system also posts `SelectSystem`, and the desktop map moves to it. Like `JoinComms`, it
names something the app already has rather than carrying anything the app acts on blindly.
