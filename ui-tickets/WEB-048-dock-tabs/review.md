# WEB-048 review cycle

**Status:** Delivered
**Branch:** `feat/web-048-dock-tabs`

One dock with tabs rather than one dock per kind: three panels side by side would leave the map a
sliver, and they are read one at a time anyway. Every kind docks when there is a map pane to dock
into, and floats when there is not, which is the page with the map switched off.

A tab per open window, the newest opened becomes the showing one, and each tab carries its own close.
A window created before the map pane existed loses the close button it was given as a floating one on
the way in: the tab has one, and two × in the same corner is one too many.

Only the showing pane takes part in the layout; the rest keep their content for when their tab comes
back, which is what makes switching between a route and the system it runs through free.

`after/dock-with-tabs.png`.
