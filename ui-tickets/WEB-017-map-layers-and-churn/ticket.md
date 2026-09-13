# WEB-017 &mdash; Every pane republished on every tick, the map was mirrored and laggy, and it was missing most of its layers

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `web/{map,snapshot,publish,facts}.rs`, `assets/map.js`, `assets/panes-*.js`, `app.js` |
| **Reported by** | user, running the installed build |

## Symptom

- Grid view updates its panes far too often, and each update resets the scroll, "breaking it".
- Map zooming is laggy; zoomed in, the dots and their highlights are far too big.
- The map is vertically mirrored.
- The map is missing most of what the app shows: bridges, upgrades, wormholes and so on.
- Alerts repeat the severity and the age above a card that already carries both.
- Fleet pings show a "matched … rule" line the app never shows. That was invented.
- Card padding is wrong.
- Distances stay in km when they should read as AU.
- zKill cards carry a redundant trailing `KILL` tag.
- Disabling a pane should remove it from the header bar, not dim it.
- "Join op" opens the Mumble link on whatever device is looking at the page.

## Measured

On the running instance, with nothing happening:

| | |
|---|---|
| Publishes in 12 seconds | **70** |
| Ticks in that time | 24 |
| Panes | 4 |

Every pane, every tick. The revision check that exists to prevent exactly this was doing nothing.

## Cause

**The churn.** `hash_of` serializes with `serde_json` to decide whether a pane changed. A `HashMap`
serializes in iteration order, and each instance gets its own hash seed, so two maps with identical
contents hash differently. The publisher rebuilds `resolved_pilots` and `last_ship` from scratch
every tick, so they never compared equal and every pane was always "changed".

**The mirror.** `map::project` draws with `center.y - (z - mid_z)`: screen y runs opposite to z.
`web::map::build` emitted z unflipped.

**The zoom.** Radii and strokes were in map units, so they scaled with the zoom, and 5255 SVG
circles re-rasterise on every viewBox change.

## How to verify

Watch `/api/state?since=<current>` on an idle app: the sequence must barely move. Zoom the map and
watch the dots stay the same size. North must be up. A fix would be WRONG if it cut the churn by
publishing less often rather than by comparing correctly.
