# UI-071 &mdash; A routed bridge is drawn twice, alternatives all read the same, and the expiry warning fires on a setting

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `app.rs` map, `web/route.rs`, `assets/{map,dialogs}.js` |
| **Reported by** | user |

## Symptoms

1. "In both app and web: If a bridge is part of a route, the arc should no longer be drawn as one
   line. The animated line should take precedence."
2. "Choosing alternative routes in the in-game map route planner does not actually update the route"
3. "The wormhole warning for saving routes should only show if the route actually contains a wormhole.
   This never applies for jump only routes"

## Cause

1. The bridge layer draws every ansiblex, and a route draws its own bridged legs on top. Two lines on
   one hop, and the one underneath says nothing the one on top does not.

2. It does update. Every button was labelled with the leg's jump count, and by construction every
   alternative for a leg has the *same* jump count, so all of them read "3j" and picking one looked
   like it had done nothing. The route underneath was changing the whole time.

3. The warning was keyed on `route_via_wormholes`, the setting, rather than on whether this route flies
   through a hole. With the setting on, every save warned, including jump routes, which cannot use one.

## How to verify

For 2, a leg with two ways to fly it: the buttons have to be distinguishable before anyone can tell
whether picking works.
