# WEB-042 &mdash; The jump planner picks any shortest path, and the route window has no hull or skills

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `jumproute.rs`, `web/route.rs`, `assets/dialogs.js`, `app.rs` |
| **Reported by** | user |

## Symptoms

1. "Routing from a waypoint will replace all waypoints and the destination after those."
2. "Verify that the jump planner actually chooses the shortest ly distance path between the systems
   and minimum jump amount."
3. "The window for the jump route should allow the user to choose the hull he is using to jump
   (changing max jump distance) and set the JDC and JFC skill (default to 5). Show the fuel cost as
   well for each jump, as well as the expected jump fatigue and reactivation timer after each jump."
4. "The wheel in the app looks pretty bad, it shows 4 boxes, containing a button each and they
   overlap each other."
5. "The in-app routing feature buttons seem to not be working"
6. "In-app map: Choosing the jump route option should open the jump route planner and interface with
   it."

## Cause

2. `shortest_path_pref` is a breadth-first search, so it minimises jumps and nothing else. Among the
many paths of the same length it returned whichever the queue reached first, which on a dense map is
routinely several light years and a lot of fuel worse than the best one. So: minimum jumps, yes;
shortest distance, no, and only by accident when it happened.

3. The endpoint hardcoded a titan at JDC V and `Hop` carried distance and nothing else.

4. Four `Area`s, one per option, each with its own popup frame.

5. The menu was cleared on `pointer.any_pressed()`, which fires on the way down: the button was gone
before its own click, on release, could land.

6. The map showed a second, worse copy of a planner the app already has as a panel.

## How to verify

For 2, a case where a breadth-first search demonstrably takes the longer of two equal-length paths.
Asserting only that the returned path is short enough would pass with the bug present.
