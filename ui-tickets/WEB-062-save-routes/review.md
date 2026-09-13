# WEB-062 review cycle

**Status:** Delivered
**Branch:** `feat/web-062-save-routes`

## What a saved route is

The whole thing: kind, anchors, avoid list, titans, the titan flags and the ship settings. A route is
the anchors *and* what you told the planner about them, and one that came back without its avoid list
would be a different route with the same name.

Saved in the app's settings rather than the browser, so a route built on the phone is there on the
desktop, which is most of the point of building it on the phone.

## Expiry

A route planned through a scanned wormhole is dropped a day after saving. The chain it was planned on
is hours old at best; a day later the route is not a route, and serving it silently is worse than
losing it. The save asks first, in those words.

Pruned where it is read rather than on a timer, so there is no background job whose failure mode is a
stale route, and `only_wormhole_routes_expire` pins both halves: a gate route from last week is still
a gate route, a day-old wormhole route is gone, a minute-old one is not.

## The height

`.rhops` had an 18rem cap that the docked overrides beat and the floating window did not. It has no
height of its own now: the box it is in decides, which is the dock when docked and the viewport when
floating.
