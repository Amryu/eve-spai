# WEB-073 — titan route: the alternatives are offered twice, and the second row is dead

> "Web map: Titan route - The alternate route buttons are duplicated. Only the first pair seems to
> work"

`chain` builds a titan route's alternatives from the **last leg's** options, because where the titan
bridges from changes the whole route rather than one hop of it:

```rust
if kind == "titan" && legs[last].options.len() > 1 {
    out = (0..legs[last].options.len()).map(|k| assemble(...)).collect();
}
```

Those options then travel to the client twice: once as `out`, drawn as the route's option tabs, and
again inside `legs[last].options`, drawn by the per-leg switcher. Same options, same labels, two
rows.

Only the first row worked, and necessarily so. Once `out` is built that way, the leg is chosen by
`routeAt` and `pick[last]` is never read again — so the second row wrote `legPick[last]`, refetched,
and got back exactly the route it already had.

## Fix

`LegChoice` gains `whole_route`, set on the leg whose options became the route's own. Both clients
skip the switcher for it. Marked on the server rather than re-derived in each client: the rule is a
property of how `chain` assembled the answer, and two clients guessing it is how they drift.

The app had the same duplicate from the same source and is fixed with the same flag.
