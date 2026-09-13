# WEB-050 review cycle

**Status:** Fixed
**Branch:** `feat/web-050-longpress`

The press sets a flag that the next click spends. Suppressing by distance would not work: a long press
that never moves is exactly the gesture, so `moved` is zero and the click looks like a tap.

The menu now holds its own dismiss function and calls it before opening another, which takes the
listener with the element instead of leaving it behind.

By hand only: the screenshot harness has no touch, which GAP-011 already records.
