# WEB-054 review cycle

**Status:** Fixed
**Branch:** `feat/ui-054-route-panel`, with the in-app panel

## The list

The server sends the avoid list back named, because the ids alone are not something anyone can check:
"avoiding 3 systems" is only useful if you can see which three. The line expands into the list, each
entry removable, permanent ones marked as such and removed through the app rather than locally.

Collapsed by default: a list that is always open costs the hops their room, and most routes avoid
nothing.

## The dock

A fixed share of the pane rather than content-sized with a cap. The dock holds tabs, and a box that
resizes when you switch between them moves the map every time you look at something else.

## Touch

The avoid button, the window close and the tab close all get real targets where the pointer cannot
hover. The avoid button is also visible at rest there, since "appears on hover" is not a thing a
finger can do.
