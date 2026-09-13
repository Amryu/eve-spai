# WEB-026 &mdash; Names sized off the pane, markers at every zoom, window still in the corner

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js`, `assets/dialogs.js` |
| **Reported by** | user |

## Symptoms

1. "Names are way too big now. Names should scale with the zoom." &mdash; WEB-025 sized names off the
   pane alone, so the enlarged size applied the instant they appeared.
2. Follow-up: "The initial size of the system and icon names should be the same as before the size
   change (at the minimum zoom required to make system names show) and then scale up to the new size
   as we approach maximum zoom", and "The scaling must feel natural, in proportion with the zoom".
3. "icons should only be showing if system names are showing".
4. "smoothen the zoom (it should still be fast, just like 50-100 ms of easing)".
5. "The default position of the system window on the web map is STILL covering the filter buttons row
   at the top. Fix it already. It should default to be a bit lower, actually where the map canvas
   starts." &mdash; third report of this.

## Cause

1&ndash;2. `nameSize = 13 * uiScale` with `uiScale` from the canvas size. One number, no zoom term.

3. Markers were drawn whenever their layer was on, gated only by `r >= 1.8` for upgrades, which as
measured is true at every reachable zoom.

4. `zoomAt` wrote `view` and painted once, so a wheel notch was a 1.2&times; jump between frames.

5. `place()` ran exactly once, as the window opened, and gave up silently when it could not measure
the canvas. The geometry is fetched, so a window opened before it lands has no canvas to measure,
and nothing ever came back. Reproduced in `before/`: the window sits at the CSS corner, covering the
map pane's own header and Layers row.

## How to verify

`before/` and `after/` are the same URL. A fix to 5 would be WRONG if it only moved the CSS default:
the window has to end up at the canvas whenever the canvas exists, not at a guessed offset.
