# WEB-028 review cycle

**Status:** Delivered
**Branch:** `web/web-028-layout-and-jabber`, shared with WEB-029, WEB-030 and UI-052

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693, up 3 with WEB-029's |
| **Follow-ups** | none |

## The budget

Four cells, and the grid is 2x2. A spanned pane costs two, everything else costs one, and `fit()`
switches off whatever does not fit, from the far end of the user's own order. `protect` is whatever
they just asked for, so switching on a fifth pane or widening a fourth gives them the thing they
clicked and takes the cost out of the other end instead of refusing.

Columns and tabs take four as well, which is the same number said a different way.

Rows are the cells actually spent, not a fixed two, or two panes would sit in the top half of an
empty grid.

## Drag

Pointer events, not HTML5 drag and drop, which has no touch story and is the reason the tab strip
next to this already works this way. The pane being carried dims; the pane under the pointer is
outlined and says **Swap**, because an outline on its own reads as "selected" rather than as "these
two trade places".

A drop swaps, rather than inserting. In a four-cell grid an insert is a shuffle of everything after
it, and there is no visual room to show which gap you are aiming at.

## Chrome

The handle and the span button are injected into each pane's heading from `afterRender` rather than
written by each pane's renderer: a pane draws its contents and should not have to know it lives in a
grid. Writing identical HTML is skipped, or the node a drag is holding would be destroyed under it.

## Persistence

`span` joins `mode`, `order`, `active` and `off` in the same `localStorage` blob, validated on load
the same way: an unknown pane name or a span value that is not `wide` or `tall` is dropped rather
than trusted.

Jabber ships off. Five panes do not fit in four cells, and a pane switched off by the budget without
the user asking reads as one that is broken.

## Verified

`after/grid-with-controls.png`: 2x2, a handle and a span button on each pane.

`?panes=intel,jabber` and `?mode=grid` are the deep links this needed to be screenshottable at all,
and they are worth having anyway: a link can now carry a layout.
