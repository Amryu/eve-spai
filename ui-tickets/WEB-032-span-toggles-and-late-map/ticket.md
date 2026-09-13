# WEB-032 &mdash; Re-adding a pane disables another, one cycling span button, map broken when added late

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/layout.js`, `assets/map.js` |
| **Reported by** | user |

## Symptoms

1. "Re-adding a view should shrink another one if necessary to fit"
2. "Also the widen to two rows button should always be there alongside the widen to two columns.
   It's only either or for both of these. Clicking the same button again will shrink it again to 1x1"
3. "Adding the map after initialization seems to add it in a broken state"

## Cause

1. WEB-028's `fit` only ever switched panes off. Switching a pane back on while another was spanned
   cost a whole pane to pay for someone else's extra cell.

2. One button cycling none &rarr; wide &rarr; tall &rarr; none, so reaching "tall" meant passing
   through "wide" and rearranging the grid on the way past.

3. `fit()` in the map reads `canvas.clientWidth || 1`. A pane that is switched off has no size, so
   `k` came out as the whole extent per pixel and `fitted` was set, permanently. When the pane was
   finally shown, the map was somewhere off the edge of a blank canvas and nothing ever refitted it.

## How to verify

Switch the map off and on again in grid mode. A fix would be WRONG if it only refitted on a window
resize: the pane gains its size without the window changing at all.
