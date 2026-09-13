# UI-067 &mdash; The in-app route panel fell out of the dock, and its arcs do not move

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `app.rs` map and right dock |
| **Reported by** | user |

## Symptoms

"The in-app map should have the same animations as the web app map (the jump route is not animated
here). Also, the jump route should still be docked to the right with the system. Right now it is still
lacking the context action button and any highlights or the like the web map docked route plan window
has"

## Cause

The route panel was only ever reachable through the right dock's **Mode** tab, and that tab only
appears when `map_mode` is not Standard. UI-063 stopped the map switching into a jump-plan mode when a
route is picked, so the tab stopped appearing and the panel with it. What was left was the separate
floating route window, which is the old one without the row actions or the waypoint tinting.

The arcs were a second thing: UI-063 gave the gate legs the crawling dashes the map already used for
its travel route and left the jump and ansiblex arcs solid, so half the route moved and half did not.

## How to verify

Pick a jump route with no map mode set. The panel has to be in the dock next to the system, with the
actions button on every row.
