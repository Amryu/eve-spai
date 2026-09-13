# WEB-039 review cycle

**Status:** Delivered
**Branch:** `feat/web-039-map-routing`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 694 to 696, two new |
| **Follow-ups** | none |

## One implementation, two maps

`web::route` takes a `&geo::Systems` and a `&[MapSystem]` and nothing web-specific, so the desktop map
calls the same three functions the phone's endpoint does. Two maps that answer "what is a titan route"
differently would be worse than either answer.

## The titan calculation

Lifted from the rescue planner's `best_jump_off`, which asks the question the other way round for the
same reason: the fewest **gate** jumps from the target that a titan can still reach, not the system
that happens to be nearest on the map. A system four light years out and twelve gates from the target
is a worse answer than one six light years out and two gates from it, and picking by distance gets
that backwards every time. `nearest_matching` returns the whole ring, which is exactly the "more than
one option" case, so ties become the option list rather than an arbitrary pick. Within a ring the
shorter jump wins: same arrival, less fatigue and less fuel.

## The gesture

A drag that starts on a system is a route; a drag that starts anywhere else is a pan. The map is
mostly empty space, so this needs no modifier and costs panning nothing.

On the desktop the hit test runs against **last frame's** positions. This frame's have already been
moved by the pan that the same drag started, so testing against them means pressing on a system and
getting a pan.

Snapping is the same `nearest` the click uses, at a slightly larger radius, because a drag ends less
precisely than a click.

## The readout

Light years, gates, and gates-with-bridges, and the bridged line only when it differs. Three numbers
because they answer different questions and are often far apart.

In the browser the two gate figures come from one breadth-first sweep per drag, over the edges the map
already has, so every hover after that is an array lookup. Asking the server would be a round trip to
compute something from data already in memory.

## The windows

The page's route window takes the system window's corner, as asked. The desktop gets an equivalent
one, with the option switcher when the titan search found several ways in.

## Verified

`cargo test --bin eve-spai`, 696, including two new ones on the route builder: a route to the system
you are already in is a single hop rather than nothing, and every hop carries a real name rather than
an id that fell out of the graph. `cargo check --workspace --all-targets --all-features` clean.

The gesture itself is by hand on both maps: the egui harness drives clicks, not drags across a canvas,
and the web shot harness has no pointer at all.
