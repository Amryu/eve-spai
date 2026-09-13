# WEB-024 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-024-float-placement`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 690, unchanged: canvas drawing, which no test can see |
| **Follow-ups** | none |

## One row per system, not one per layer

Each layer now records its markers against a system rather than drawing them, and a single pass draws
each system's row centred above its dot. That is what makes them line up: four layers drawing
independently had no way to know what the others had already put there, and three of them used the
same offset.

Markers are 16px and names 13px, both a size up.

## Where the window goes

Aligning to the canvas top is not low enough, because on a narrow pane the layer panel overlays the
top of the canvas. It now clears whichever of the toolbar and the open panel reaches furthest down,
and only counts a control that actually overlaps the map: a toolbar sitting above the canvas is
already clear of it.

It also bails out when the canvas is off screen, which is where the map sits in tabs mode while
another pane is showing. Positioning from a rect that is nowhere near the viewport was how it ended
up over the intel pane in a 820px screenshot while I was checking this.

Dragging it still pins it: once moved, it stays put.

## Verified

`after/markers.png`: the campfire above the camped system and the spiral above the wormhole end, each
a single centred row, with the route running between them.
