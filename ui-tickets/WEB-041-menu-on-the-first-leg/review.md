# WEB-041 review cycle

**Status:** Fixed
**Branch:** `feat/web-041-extend-without-menu`

The kind is chosen once, at the first drag, and held. A drag off the current destination is adding a
waypoint to a route that already has an answer, so it extends silently; a drag from anywhere else is
a new route and asks again, which is also what makes the question answerable a second time.

Both maps, both from the same rule.
