# WEB-049 review cycle

**Status:** Fixed and verified
**Branch:** `feat/web-049-waypoint-rings`

## The mark

Two rings and a tinted disc, in the route's own amber. One thin ring is lost among the rings this map
already draws for characters, camps and the hovered system; a filled target is not.

## The route

Announced rather than called back. The map now hears about a route through an event, which also means
this module does not have to import the map: dialogs already imports the pane renderers and the map
imports dialogs, so a callback was the only way round a cycle.

An event has no replay, though, and a route opened from a deep link is announced before the map has
wired its listener. So the map reads the live binding once when it wires and follows the event after
that. Closing the route clears both.

## Verified

`after/waypoint-rings.png`: the route in animated amber dashes with both anchors ringed, opened from
a link with nothing clicked. Before this the map drew nothing at all from a link.
