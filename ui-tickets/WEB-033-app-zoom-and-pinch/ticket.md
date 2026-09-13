# WEB-033 &mdash; A different zoom to the app's, and a pinch that only scales

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js`, `assets/layout.js` |
| **Reported by** | user |

## Symptoms

1. "Implement the same map zoom as in the app in the web. Also make sure mobile zooming is properly
   supported"
2. "The map loaded it's 2 column size, but disabling the colspan did not shrink it."

## Cause

1. Three differences from the app, each felt:
   - a fixed 1.2&times; per wheel event, where the app uses `zoom * exp(scroll * 0.003)`, continuous
     in the scroll;
   - `deltaY` used raw, so the same gesture zoomed six times as far in Firefox, which reports lines,
     as in Chrome, which reports pixels;
   - limits of five times out and a thousand times in, against the app's `clamp(0.7, 60.0)`.

   The easing was fixed at 180ms, which a stream of trackpad deltas turns into permanent lag: each
   event restarts a curve that is 2% along by the next frame.

   Pinch scaled about the midpoint but ignored the midpoint moving, so the map slid out from under
   the hand doing it, and a pinch could end as a tap because nothing counted it as movement.

2. The map's two-column default was computed on every apply, so clearing the span made the rule put
   it straight back. The button worked and looked broken.

## How to verify

The wheel on a trackpad and the wheel on a mouse, in Firefox and in Chrome. A fix would be WRONG if
it matched the app on one of those and not the others.
