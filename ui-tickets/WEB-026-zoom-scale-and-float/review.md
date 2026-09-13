# WEB-026 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-026-zoom-scale`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 690, unchanged |
| **Follow-ups** | none |

## Size: the pane sets the ceiling, the zoom decides how much of it is earned

`uiScale` stays, but it is no longer the size. It is the most a name or marker is allowed to reach,
and `grow = 1 + (uiScale - 1) * zoomT` walks from the pre-WEB-025 13px and 16px at the moment names
appear to that ceiling at the innermost zoom stop, which is what the user asked for to the word.

`zoomT` is logarithmic in `k`, not linear, because zoom is: every notch is a constant ratio, so a
constant share of the ramp per notch is what reads as proportional. Linear in `k` would have spent
the whole ramp in the first notch and then flattened.

It is measured in `k` against the two zoom stops rather than in the on-screen span, so a wider pane
does not quietly move the ends of the ramp.

## Markers follow the names

A marker annotates a system you can identify. Zoomed out past the names it is a glyph over an
anonymous dot, and a thousand of them hide the map they are drawn on. So one gate, `namesOn`, decides
both. The `r >= 1.8` upgrade gate is gone: measured, `radius()` is pinned at its 3px cap across the
entire reachable range, so that condition was never false and was never doing anything.

Tied to the zoom threshold, not to the Layers &rarr; Labels toggle. Each icon layer keeps its own
switch; turning off names should not silently take the camps with it.

## Zoom easing

`view` is what is drawn, `target` is where the gesture put it, and the drawn one decays towards the
target with a 80ms time constant. Exponential rather than a fixed curve, so a second notch during the
first just moves the target and the same decay carries on: a fast scroll never queues a backlog.

Pan is deliberately **not** eased. A drag has to track the pointer exactly, so it writes both views
at once; easing it would read as lag, which is the opposite of the ask.

`zoomAt` compounds off `target`, not off the drawn view, so notches during an easing zoom add up
instead of fighting the animation back to where it started.

## The window, for the third time

Placement was a single shot that failed quietly. It is now a placement that keeps trying: it retries
for about a second when it cannot measure, it re-parks on resize, and the map pane fires `spai:map`
when it creates its canvas, which closes the timing hole exactly rather than hoping a delay covers it.

`before/float-in-the-corner.png` and `after/float-at-canvas-top.png` are the same URL. In the after
shot the pane's own "Map" heading, Layers button and "3 of 3 systems" hint are all visible and the
window starts at the top of the canvas.

Getting that evidence needed `?dlg=system/<id>`: the hash holds one route and the shot needs two, the
pane and the dialog. Kept, because a link to a system is worth sending.

## Verified

`cargo test --bin eve-spai`, 690 passed. Easing and the zoom ramp are by inspection in the browser:
the fixture map is three systems, which never reaches the names threshold, so it cannot show them.
