# WEB-049 &mdash; Waypoints are not marked on the map, and a deep-linked route is not drawn at all

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js`, `assets/dialogs.js` |
| **Reported by** | user |

## Symptom

"The waypoint systems should also have a highlight circle on the map."

## Cause

Two, and the second is why the first could never have been seen for what it was.

The ring existed since WEB-046, drawn one pixel thin in the same amber the route uses, on a map that
already rings your characters, camped systems and whatever is hovered. It read as another of those.

The route itself was only drawn when the map passed a callback into `showRoute`. A route opened from
a deep link passes none, so it was never drawn, which is also why this could not be checked in a
screenshot.

## How to verify

A route on the map with its endpoints ringed, from a link, with no clicking.
