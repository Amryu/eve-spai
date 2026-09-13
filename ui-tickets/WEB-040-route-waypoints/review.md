# WEB-040 review cycle

**Status:** Delivered
**Branch:** `feat/web-040-route-waypoints`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 696 to 697, one new |
| **Follow-ups** | none |

## Chaining

The anchors are the systems the drags named, in order. A drag that starts on the current destination
appends; a drag that starts anywhere else replaces, which is the only way to abandon a route without
a separate control for it.

`chain` walks the anchors and joins the legs, dropping each leg's first hop because it is the
previous leg's last. `a_chain_joins_at_the_waypoint_without_repeating_it` is the regression: the
waypoint appears exactly once in the path, the hop list and the path stay the same length, and the
detour is never shorter than the direct route.

Only the last leg can have alternatives, which is what the option list is for. A titan route chains as
gates up to that leg, because a titan route *is* one jump and then gates; chaining several jumps is
what the jump route already does.

Both maps share this, the same way they share the rest of `web::route`.

## The dialog

A fixed height in both, not a cap. A dialog that shrinks as the search narrows moves the row you were
reaching for out from under the pointer, which is worse than some empty space.

`after/start-dm-fixed-height.png` is the desktop one with three matches, holding its size.
