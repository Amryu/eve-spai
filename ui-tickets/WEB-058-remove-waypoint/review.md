# WEB-058 review cycle

**Status:** Fixed
**Branch:** `feat/web-058-remove-waypoint`

An anchor is a choice the user made, so the row it sits on is where taking it back belongs. Named for
what it is at that position, destination or waypoint, in both clients.

Not the start. A route has to begin somewhere, and an anchor list without one means nothing; the app
also refuses the last removal that would leave fewer than two.
