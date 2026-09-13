# UI-070 review cycle

**Status:** Fixed
**Branch:** `fix/ui-070-ingame-route-arcs`

The bridge leg arcs, animated, through `polyline_flow`, which is what UI-067 added so an arc can crawl
the way a straight leg does.

This was the last of four places drawing a route leg, and the fourth to need the same fix. The travel
route already arcs its bridges, though solid rather than crawling, which is that planner's own style
and not something to change here.

Not installed: the last instruction on the subject said not to, and 0.8.0 has just been released, so
putting a dev build over it is not something to do without being asked.
