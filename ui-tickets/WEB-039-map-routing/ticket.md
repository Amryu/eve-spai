# WEB-039 &mdash; Drag from a system to route to another, on both maps

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/route.js`, `assets/map.js`, `assets/dialogs.js`, `app.rs` map |
| **Reported by** | user |

## Ask

"Both maps: Starting to drag on a system will make a line appear to the cursor. If that line is
hovered on another system, show the ly, gate and jump bridge distance as a hovering tooltip next to
the system. Make sure the line snaps to the system when close enough. Letting go on that system is
going to open a radial menu with 4 options: Gate Route, Jump Route, Combined Route, Cancel - Gate
Route will create a regular route and set the destination for the selected character. Jump Route will
not set a destination, but instead show a jump route. The web app will need a new window (replacing
the system window temporarily), that will show the list of jumps (basically the jump planner from the
app). The combined route (call it titan route) will try to find the best jump to get to the target
system with as few jumps as possible. If there are more than one possible options, show a window that
allows switching between the options). (This is basically the same as the calculation from the rescue
feature with the titan)"

## Gap

Routing was reachable only from the travel planner and the map's context menu, both of which want a
system named rather than pointed at. Nothing answered "how far is that from this" without leaving the
map.

## Acceptance

The same gesture and the same three answers on both maps. A titan route that picks by distance rather
than by gate count would be WRONG: the rescue tests already pin that distinction.
