# UI-055 &mdash; The in-app planner is a jump-only form with the route at the bottom

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `app.rs` jump plan panel, `web/route.rs`, `assets/dialogs.*` |
| **Reported by** | user |

## Asks

1. "The in-app jump route planner is a bit bloated compared to the web version now. Re-design that
   side-bar to be more compact. Highlight waypoint systems, add ways to modify avoidance, etc. - Also
   add a new menu for regular routes like the web has now and the 'titan jump' route. - If there is
   intel for a system, open that intel in a dialog as well to be able to inspect it."
2. "Why is the avoidance button on a new line on desktop? Fix that. If a jump is different, because an
   avoided system would have been hit the system should state this in the jump/gate list."
3. "In the jump/gate list: The badge for systems should have a stable width ... at least null sec
   system names."
4. "If docked on the right, the jump/gate list may take up the remaining height of the box"

## Cause

1. The panel was built for one kind of route and put the hull picker, both skills and an ESI button
   above the first hop. The route, which is the subject, started a third of the way down.
2. The costs and the warning each take a full row, so the avoid button after them wrapped.
3. Proportional names in a left-aligned badge.
