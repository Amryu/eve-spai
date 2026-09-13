# WEB-046 &mdash; No avoidance, no alternatives, no way to see which systems you named

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/map.js`, `assets/dialogs.*`, `settings.rs`, `app.rs` |
| **Reported by** | user |

## Ask

"Route planner: Add avoidance lists (separate for gate and jump routes). Avoided systems can be made
permanent or just for this route that is currently being planned. 'Waypoint' systems should be
highlighted on the map and the list of jumps. Every waypoint/destination should offer a few
alternative routes (same amount of jumps, sorted by total jump distance)"

Plus: "The routes seem nice, however the styling of them differs from the in-app one. I would like them
to be animated dashed lines" and "The web map needs to respect the 'use wormhole in routes' setting and
expose it as well."

## Gap

A route was take it or leave it. Nothing said which systems on it the user had actually named, there
was no way to say "not through there", and the several equally short ways of flying a leg were
invisible because only one of them was ever computed.

## Acceptance

Two avoid lists, because a system you will not gate through is often perfectly fine to jump over.
