# WEB-033 review cycle

**Status:** Fixed
**Branch:** `web/web-033-app-zoom`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693, unchanged |
| **Follow-ups** | pinch and trackpad feel are by hand; the harness has no gestures |

## The curve

`exp(-px * 0.003)`, which is the app's `exp(scroll * 0.003)` with the browser's sign. Continuous in
the scroll rather than a step per notch, which is what makes a trackpad feel like a trackpad and not
a ratchet.

Normalised to pixels first: `deltaMode` is lines in Firefox and pixels in Chrome, a factor of sixteen
between two browsers running the same page. Clamped at 240px so a flick of inertia is a fast zoom
rather than a teleport.

## The limits

The app's, `clamp(0.7, 60.0)`, held as multiples of the framed universe so the two maps agree
whatever size the pane is. `fitK` is recorded by the universe fit.

Framing a small region is allowed past the 60&times; limit, and the clamp never tightens past where
the view already is, because being unable to zoom back out of a region you just framed would be worse
than the limit being exact.

## The easing

Duration is now proportional to the size of the jump, capped at 180ms. A wheel notch works out at
about 40ms, a trackpad's stream at two or three, a reframe at the full 180. Fixed at 180 a stream of
small deltas never converged: each event restarted a curve that was 2% along by the next frame, so
the view sat permanently behind the fingers.

## Pinch

Two fingers pan as well as scale: the midpoint is tracked and its movement applied as a drag, on the
drawn view, the target and the anchor of any zoom in flight. It also counts as movement, so lifting
off no longer lands as a tap on whatever was under the last finger.

## The span that would not clear

The two-column default for the map is now seeded once, as a real stored value, and any deliberate use
of a span button retires it for good. Computed on every apply it was not a default at all, it was a
rule, and clearing the span just made the rule put it back.

## Verified

`cargo test --bin eve-spai`, 693. `after/mobile-map.png` is the map at 390px in tabs mode. Pinch
itself is by hand: the screenshot harness has no gestures, and GAP-011 already records that.
