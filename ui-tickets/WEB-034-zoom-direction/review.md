# WEB-034 review cycle

**Status:** Fixed
**Branch:** `web/web-034-zoom-direction`

One character. `k` is map units per pixel, so a positive `deltaY`, which is a scroll away from the
user and therefore zoom out, has to make it larger. The comment at the call site now says which of
the two conventions is in play, since getting it backwards took two sign flips that each looked
right on their own.
