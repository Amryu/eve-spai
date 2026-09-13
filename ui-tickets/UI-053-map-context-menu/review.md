# UI-053 review cycle

**Status:** Fixed
**Branch:** `feat/ui-053-map-menu`

The same menu the browser has, in the same order, plus Favourite, which is the one entry the browser
has no use for.

What went: Travel-mode start/destination/waypoint/avoid, the ESI destination and waypoint entries,
"Mark hole dead", the three jump-plan entries, Show Info, and the capitals/supers dock entries. Every
one of them either duplicates the route drag or belongs to a mode with its own panel.

The route entries share the drag's own helpers rather than editing the anchor list themselves, so the
two ways of building a route cannot end up meaning different things. `Set as Destination` on a route
with only a start completes it; on a longer one it replaces the far end and leaves the waypoints
where they are, which is what the browser does.

Avoidance is here for both lists, and "Stop avoiding always" only where there is something to stop,
matching WEB-047.
