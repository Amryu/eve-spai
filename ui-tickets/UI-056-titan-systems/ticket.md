# UI-056 &mdash; The titan route guesses where the titan is, and every row grows another button

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/route.rs`, `jumproute.rs`, `app.rs`, `assets/{map,dialogs}.*` |
| **Reported by** | user |

## Asks

1. "Titan route only: Add a button to each system, and the context menu, to set a system as the titan
   system (even systems that are not currently on the route). If a titan in that system provides no
   faster route inform the user and just show the regular gate route."
2. "The user may also add multiple titan systems, choose whichever is better. Titan systems should get
   a special highlight on the map"
3. "(Titan systems are only saved with the route, not permanently for everything)"
4. "For each jump/gate, add an 'actions' button (use a symbol) so the rows don't get crowded with
   buttons."
5. "Every system on a path should have a button to list alternatives (for jumps only). If a user
   selects an alternative, it is being added as a new waypoint inbetween."
6. "If 'Titan is in starting system' is selected, allow the titan to jump itself and bridge from it's
   new location as well. Draw a special jump route animated line ..."
7. "In the gate list, use 'ansiblex' instead of 'bridge'."
8. "Jump route planner: Zarzakh is never a viable system to jump into or from"
