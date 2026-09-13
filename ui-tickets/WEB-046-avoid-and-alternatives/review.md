# WEB-046 review cycle

**Status:** Delivered
**Branch:** `feat/web-046-avoid-and-alternatives`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 700 to 701, one new |
| **Follow-ups** | the in-app context menu still carries its old entries |

## Avoidance

Two persistent lists in settings, one per kind of route, added with `serde(default)` and never
retyped: a changed field type fails the whole parse and resets every setting there is.

The permanent half lives in the app and the page only ever sends the this-route-only half, so there
is one authority for what is permanently avoided and a phone can still add to it through an action.

The endpoints are exempt. Avoiding the system you are standing in, or the one you asked to go to,
would mean no route at all, which is a worse answer than an honest one.
`an_avoided_system_is_not_routed_through` pins both halves of that.

For a jump route, avoidance takes the systems out of the graph rather than filtering the result: a
path through a banned system is not a worse path, it is not a path.

## Alternatives

Per leg, not per route. The best path, then the best path with each of its intermediate systems
banned in turn, keeping the ones that take the same number of jumps: the cheap half of Yen's
algorithm, which is enough when the question is "what else costs the same" rather than "rank every
path there is". Sorted by distance, so the list reads as "these all cost the same, shortest first".

The window assembles nothing: it sends which alternative it wants per leg and the server joins them,
because joining legs is where the duplicated hop lives and one implementation of that is enough.

## Waypoints

`Hop.anchor` marks the systems the user named, as opposed to the ones the route passes through. The
map rings them and the list outlines them.

## Wormholes

The app's setting is honoured and shown. The chain is passed as extra edges to `route_with`, which is
how the app's own planner takes them, rather than as a flag: a second way of expressing "there is a
hole here" is a second way to be wrong. The switch writes back to the app rather than being a
per-device preference, because it changes what the desktop plans too.

## Dashes

The picked route is animated dashes at the app's own 6/6 spacing. A static line is hard to pick out
of a map already full of lines, and the crawl says which way round the route runs. The map keeps
painting while one is on screen, which is the cost of an animation and is bounded by the route
existing.
