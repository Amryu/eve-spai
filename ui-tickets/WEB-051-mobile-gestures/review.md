# WEB-051 review cycle

**Status:** Fixed
**Branch:** `feat/web-051-mobile-gestures`

A second pointer cancels whatever the first was doing and the gesture becomes a pinch. Reordering the
branches would not have been enough: the line was still being dragged underneath, so the first finger
would have gone on fighting the zoom for the map's origin.

On a touch screen a drag off a system is a route only once there is a route, which the long press
starts. Before that, a drag anywhere is a pan, which is the only thing a phone can afford to make
conditional: a mouse always has somewhere else to press, and a phone showing a dense map often does
not.

By hand only: the screenshot harness has no touch.
