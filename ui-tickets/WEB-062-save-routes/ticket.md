# WEB-062 &mdash; No saving or loading routes in the web, and the hop list scrolls early

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `settings.rs`, `ipc.rs`, `web/server.rs`, `assets/dialogs.*` |
| **Reported by** | user |

## Asks

1. "Allow saving and loading routes (routes that contain wormholes will auto-delete in 24h, warn about
   this). This feature should already somewhat exist in the app I think, the web needs it too"
   &mdash; then "The option to save/load routes is missing in the web."
2. "Either mpanel or rhops is being restricted in it's height somehow causing the route list to scroll
   earlier than it needs to."

## Cause

1. The app's two existing saved-route types are the travel planner's and the old jump planner's, and
   neither carries what a map route is made of now: a kind, an anchor list, an avoid list, titans and
   the ship settings.
2. `.rhops` carried `max-height: 18rem`, which the docked overrides beat but the floating window did
   not, so the list scrolled at a third of the height it had.

## How to verify

A saved wormhole route a day later. A fix would be WRONG if it kept it and let the route quietly be
about a chain that has since collapsed.
