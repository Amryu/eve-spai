# WEB-042 review cycle

**Status:** Fixed
**Branch:** `feat/web-042-jump-planner`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 697 to 700, three new |
| **Follow-ups** | none |

## The path

Dijkstra on `(jumps, systems you cannot dock in, light years)`, in that order, replacing the
breadth-first search.

Jumps stay first because one fewer jump is worth any amount of distance: fatigue is charged per jump,
not per light year. Docking preference keeps the priority it already had over distance, so nothing
that worked before stops working. Light years decide what used to be arbitrary.

`equal_jumps_go_the_short_way` is the regression, and it is built to fail on the old code: the longer
of the two equal-length paths is listed first in the system array, which is exactly the one a
breadth-first queue reaches first. `fewer_jumps_beat_shorter_distance` pins the priority from the
other side, with a two-hop path that is strictly shorter in light years than the one-hop answer.

## Fuel and the timers

`hop_costs` gives the per-jump bill and `route_cost` is now folded out of it, so the totals and the
lines they are read beside cannot disagree about how fatigue compounds.
`hop_costs_add_up_to_the_route_cost` pins that, including that each jump leaves more fatigue than the
one before.

Both clients show isotopes, the blue bar and the red bar per jump. The hull picker and the two skills
come from the app's own `SHIP_CLASSES`: a picker that disagrees with the planner about what a jump
freighter can do is worse than no picker. Skills default to five. The titan option stays a titan
whatever the picker says, because that is what the word means.

## The wheel

One `Area`, one disc, four buttons placed on it. Four areas with four frames overlapped each other and
read as a mistake.

And it is dismissed on release rather than on press. Clearing the menu the moment a button went down
took the button away before its own click, on the way up, could land, which is why none of them
worked: the look and the deadness were the same fix.

## The jump route in the app

Handed to the jump planner, which already has the hull, the skills, the waypoint editing and the map
overlay. A second read-only copy of it beside the real one was the wrong answer to the request.

## Waypoints

A drag off any system already on the route rewrites it from there: everything after that system goes
and the new target becomes the destination. Off the destination that is the same as appending, which
is what WEB-040 did, so the new rule replaces the old one rather than sitting beside it.
