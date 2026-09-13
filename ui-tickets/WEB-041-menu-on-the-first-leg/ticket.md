# WEB-041 &mdash; The radial menu asks again on every waypoint

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `assets/map.js`, `app.rs` |
| **Reported by** | user |

## Symptom

"(The wheel is only necessary for the first line being drawn)"

## Cause

WEB-040 made a drag off the destination extend the route, but left the menu in front of every drag.
The menu asks what kind of route this is, which is one answer per route, not one per leg, so adding a
third waypoint meant answering the same question a third time.
