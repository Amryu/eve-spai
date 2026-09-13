# WEB-025 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-025-map-scale`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 690, unchanged |
| **Follow-ups** | none |

## What changed

One `uiScale` from the canvas's smaller dimension, clamped between 1 and 1.9, applied to system
names, region names, markers, ADM and the jump-range readouts. Capped because past a point bigger
type stops helping and starts crowding, and floored at 1 so a narrow column is exactly as it was.

The dot radius is deliberately **not** scaled. That size was asked for specifically and is a function
of zoom, not of how much room the pane has.

## The half that makes it work

Scaling type alone would have traded small labels for overlapping ones. System names are now placed
rather than drawn: a name that would land on one already down is skipped, the same rule the region
labels got in WEB-020.

Candidates are sorted by distance from the centre of the view, so when names do have to be dropped,
the ones that survive are the ones nearest what the user is looking at. Sorting by array order, which
is system id, is what made whole regions go unlabelled the last time this came up.

## Also

`#pane/<name>` selects a pane on load. It makes a link point at one, and it is the only way a
load-time screenshot can reach a pane that is not the first, which is what this ticket needed to show
its own subject.

## Verified

`after/map-tab.png`: the map as a full-width tab, layer panel in the flow, markers and the region
label at the larger scale.
