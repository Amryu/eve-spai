# WEB-031 review cycle

**Status:** Fixed
**Branch:** `web/web-031-swap-span`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693, unchanged |
| **Follow-ups** | none |

The span goes with the place, not with the pane: `swap` now exchanges both, deleting rather than
writing an empty value so a normal pane does not end up with a span key that says "no span".

The shape of the grid is a property of the layout the user built. A drop says which pane goes where,
not how big the cells are.
