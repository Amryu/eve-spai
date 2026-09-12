# WEB-011 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-011-map`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified, first-pass layer set |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 640 to 645 |
| **Follow-ups** | none |

## What changed

`web::map::build` projects `store::all_map_systems`'s `x2d`/`z2d` into a 0..4096 box and emits edges
as **index pairs** into the node array, deduped `a < b`, from `neighbors_gates_only`. A system id is
eight digits and a universe has ~13.5k edges, so indices roughly halve the payload;
`edges_are_indices_on_the_wire_not_ids` asserts every edge value indexes a node that exists, which is
what stops the format drifting back to ids.

Built once in `SpaiApp::web_map_geometry`, where both halves are to hand: the store has the
coordinates, `systems` has the graph. Served with a content-keyed `ETag`, so a rebuilt SDE serves a
new tag and an unchanged one revalidates to 304.

The pane is one `<svg>` panned and zoomed by mutating its `viewBox`, with every edge in a single
`<path>`. Nodes are culled to the viewport plus a margin. Live layers ride in the snapshot and carry
system ids only: security colour from the same eleven-stop ramp, intel heat under the dot so the
security colour stays readable on top, and an accent ring where your characters are.

## A defect the screenshot caught

The first render showed four specks and no links at all. Both radius and stroke were in **user
units**, which shrink on screen as the viewBox widens: at the fitted zoom a 1.5-unit stroke is about
a tenth of a pixel.

Links now use `vector-effect: non-scaling-stroke`, so they are 1.5 screen pixels at any zoom. Radii
are computed from the measured pixel width of the box, and a zoom repaints on a debounce because it
changes both what a pixel is worth and what is on screen; a pan only moves the box and takes the
cheap path.

## Out of scope, on purpose

`map_layers_content` (`app.rs:11172`) offers ADM, sovereignty upgrades, cyno generators, jump range,
wormhole connections and the two threat layouts. None are here. This pass is security colour, intel
heat, your position and gate links. Saying so in the ticket and again here is what stops the next
round relitigating it.

`SysFlags` activity refreshes hourly upstream, so it belongs in a polled endpoint rather than a 2s
push; that is why it is absent rather than forgotten.

## Verified

`after/web-1440.png`: the fixture chain 1DQ1-A to 319-3D to 7-K5EL drawn with its gate links, systems
coloured by security with Jita cyan against nullsec magenta, and the accent ring on the system the
fixture player is in.
