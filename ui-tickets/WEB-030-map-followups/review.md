# WEB-030 review cycle

**Status:** Fixed
**Branch:** `web/web-028-layout-and-jabber`, shared

## The easing

The pan is now *derived* from the zoom rather than eased beside it. The gesture is about one map
point, the one under the cursor; `settle` solves `screen = (map - o) / k` for it at every frame, so
it stays under the cursor for the whole animation instead of only at the two ends.

A drag mid-flight moves the pinned point with it, or the next frame pulls the map back out from
under the finger.

## The rings

Gone, with the band labels. The per-system tint stays, which is the part that is true in two
dimensions: it is computed from the real 3D positions, so a system tinted as in range is in range.
