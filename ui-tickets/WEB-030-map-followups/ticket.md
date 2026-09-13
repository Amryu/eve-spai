# WEB-030 &mdash; The eased zoom fights the cursor, and the jump-range rings are a lie in 2D

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `assets/map.js` |
| **Reported by** | user |

## Symptoms

1. "The easing does not really match up with the panning and zooming caused by it."
2. "Do not show the jump range rings in the web app. This only makes some sense in the 3d map. Only
   highlight the system colors."

## Cause

1. WEB-027 eased the origin on one curve and `k` on another. The two agree at the ends and nowhere in
   between, so the point the gesture was aimed at slid across the screen for the length of the
   animation.

2. A jump range is a sphere. A circle drawn on a top-down projection takes in systems that are light
   years above or below the plane and leaves out ones that are in range, so the ring is wrong in a way
   the per-system tint is not.
