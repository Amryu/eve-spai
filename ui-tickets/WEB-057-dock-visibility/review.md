# WEB-057 review cycle

**Status:** Fixed and verified
**Branch:** `fix/web-057-dock-when-visible`

Docking now requires the map pane to be on screen, not merely present: `hidden` and `offsetParent`
together, because a pane can be out of view for either reason and neither is a place to put a window.

A window already docked when the map is switched off floats again rather than staying in a box nobody
can see, and gets back the close button it gave up to the tab strip on the way in.

`after/floats-without-a-map.png`: a system dialog with the map pane off, floating and legible. Before
this the same click drew it inside a hidden element.

Also: the system window says "Ship kills (1h)" rather than "Ship kills, last hour", which was three
words of chrome on each of three rows in a docked column.
