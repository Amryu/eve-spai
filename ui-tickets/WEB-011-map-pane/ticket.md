# WEB-011 &mdash; No map in the browser

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `web/map.rs`, `assets/map.{js,css}` |
| **Reported by** | user, remote web view |

## Gap

Intel without a map is a list of system names. The desktop map is an egui canvas and cannot be
reached from a page.

## Deliverable

`GET /api/map/geometry?region={id}` and `?universe=1`, serving nodes from `store::all_map_systems()`
(`store.rs:719`), which already carries the `x2d`/`z2d` projection the desktop map uses, quantised
into a 0..4096 box via `map::Bounds`, plus edges as **index pairs** into the node array, deduped
`a < b`, from `geo::Systems::neighbors_gates_only`. Jump bridges as a separate list. `ETag` keyed on
the SDE version, `max-age=604800`, gzipped with `flate2` when the client accepts it.

An SVG pane: one root `viewBox` mutated for pan and zoom, **all edges as a single `<path>`** rather
than thousands of `<line>` nodes, `<circle>` only for nodes inside the viewport on a debounce, labels
above a zoom threshold, `touch-action: none` with Pointer Events for pinch. Tap a system to open its
dialog from WEB-008.

Live layers ride in the snapshot on a 2s divider: your location, character positions, per-system intel
severity and newest timestamp, camps, sov colour.

## Measured

| | |
|---|---|
| Systems in the SDE | ~8000 |
| Gate edges | ~13500 |
| Universe payload, raw JSON | ~450 KB |
| Universe payload, gzipped | ~110 KB |
| A region | ~100 systems, ~6 KB |

## Notes

Region is the default view. Universe is an explicit tap, and starts as a static coloured overview
rather than a fully interactive canvas.

**Live layers never carry geometry.** That rule is what keeps a 2s push from re-sending the universe.

`SysFlags` activity (ship, pod and NPC kills, jumps, ADM) refreshes hourly upstream, so it is a
separate `/api/map/status` polled at 60s, not pushed. Per-client subscription state stays out of the
server entirely.

Out of scope for the first pass, deliberately: ADM shading, sov upgrades, cyno generators, jump range,
wormhole connections, the threat layouts (`MapLayout::Radial`, `Tree`). `map_layers_content`
(`app.rs:11172`) is the full list; this ticket implements security colour, intel heat, your location,
camps and sov colour only.

## How to verify

- `geometry_json` on a fixture `Systems`: node count, every edge deduped `a < b`, every coordinate
  inside 0..4096, region filtering correct.
- An `#[ignore]`d assertion on the real universe payload staying under a stated gzipped size, so a
  later change that inflates it is caught.
- Demo screenshots of the region and universe views on a phone, and a pinch-zoom checked by hand.
- A fix would be WRONG if it emitted one SVG element per edge, or if it put node coordinates into the
  2s live payload.
