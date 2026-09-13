# UI-071 review cycle

**Status:** Fixed
**Branch:** `fix/ui-071-route-bridges-and-picks`

## The doubled bridge

Both maps collect the bridged hops of everything currently drawn as a route, the planned one, the
travel route and the in-game destination route, and the bridge layer skips those pairs. The route's
line wins, which is the right way round: it is animated, it is in the route's colour, and it is the
one carrying information.

## The alternatives that "did nothing"

They worked. The labels did not. `leg_options` keeps only alternatives with the same jump count as the
best, so labelling them by jump count labels them all identically, and for a gate route there is no
distance to fall back on either.

Each option is now named after the first system on it that the others do not share, which is exactly
the thing that makes it a different option. "via KVN-36" instead of "3j", in both clients.

I wrote a test for the pick mechanism first and deleted it: the fixture graph is four systems, so no
leg in it has two equal-length ways to fly, and the test passed without exercising anything. A test
that cannot fail is worse than none, and the gap is real: `leg_options` has no coverage.

## The expiry warning

`RouteOption` carries whether the route actually crosses a hole, found by asking the graph whether each
step exists in its own adjacency: a step that only worked because the hole map was passed in is a hole.
A jump route sets it false outright, because a capital jump is not a wormhole whatever the setting says.

Both save paths read that rather than the setting, so the warning appears when the route is genuinely
on a clock and not merely when the user allows holes in general.
