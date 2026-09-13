# WEB-032 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-032-span-buttons`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693, unchanged |
| **Follow-ups** | none |

## Shrink first

`fit` now drops spans from the far end of the order before it switches anything off. A pane that lost
its second cell is still there to read; a pane that was switched off is gone, and trading one for
someone else's width is the worse of the two.

## Two toggles

Both buttons are always present and each is its own toggle, exclusive of the other, with the active
one showing in the accent colour and offering to go back to one cell.

## The late map

Two parts, and only fixing one would have left it broken.

`fit()` refuses a canvas under two pixels and leaves `fitted` alone, so the first paint with a real
size does the job properly instead of inheriting a view fitted to nothing. `paint()` does the same.

A `ResizeObserver` on the canvas is what notices, because the pane gains its size without the window
changing at all, so a resize listener would never have fired.

The pane's controls were missing from the map as well: it rebuilds its own heading when the geometry
lands, outside `render`, taking the injected chrome with it. It already announces that with
`spai:map`, so the layout listens and puts them back.

## Verified

`after/grid-with-both-toggles.png`: four panes in the 2x2, both span buttons and the handle on each,
and the map drawn rather than blank. Before this it was an empty rectangle in that cell.
