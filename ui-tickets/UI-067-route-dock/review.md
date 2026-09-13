# UI-067 review cycle

**Status:** Fixed
**Branch:** `feat/ui-067-route-dock`

## The dock

The route is its own tab rather than a face of the mode tab, which is what tied it to a mode that no
longer changes. Planning one opens the dock on that tab, so a route built on the map has somewhere to
be read without hunting for it, next to the system exactly as in the browser.

The tab fallback picks the first tab this dock actually has, in order, rather than bouncing between
two named ones: with three tabs the old pair of `if`s could land on an empty one.

The floating route window is deleted. It was the old panel without the row actions or the waypoint
tinting, and keeping a second, worse copy of a panel is how the two drift.

## The arcs

`polyline_flow` runs `dashed_flow` along a polyline with the phase carried forward, so an arc crawls
the way a straight leg does. Carried forward and not restarted per segment: a fourteen-point arc that
restarts the dash pattern at every sample shimmers rather than flows.

The titan's own jump crawls the other way, which is what the browser does and what says it is a
different ship going somewhere else.
