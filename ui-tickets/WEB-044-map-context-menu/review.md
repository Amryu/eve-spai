# WEB-044 review cycle

**Status:** Delivered
**Branch:** `feat/web-044-context-menu`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 700, unchanged |
| **Follow-ups** | none |

## What it offers

Entirely a function of state, which is what keeps it short: with no route, the three "Start ..."
options and nothing about waypoints; with one, what this particular system can be in it. A system not
on the route offers Destination and Waypoint; one already on it offers Remove, named for what it
actually is at that position. Clear is there whenever there is something to clear.

"Add Waypoint" inserts before the destination rather than appending, which is the difference between
a waypoint and a destination and the whole reason the menu exists alongside the drag.

## Reaching it

Right-click on a desktop, a 500ms press on a phone. The press timer is cancelled as soon as the
pointer moves, so a press that becomes a route drag never also opens a menu, and the drag is torn down
when the menu opens so the two cannot both be live.

The menu is measured before it is placed, so one opened near an edge comes back on screen.

## The pending start

"Start Gate Route" leaves a route with a start and nowhere to go, which would otherwise look like the
menu did nothing. The start gets a dashed ring until a destination turns it into a route.

## Shared planning

Both the drag and the menu go through one `replan`, so there is one place that decides what an anchor
list means and one call that draws it.
