# WEB-059 review cycle

**Status:** Fixed
**Branch:** `fix/web-059-popups-and-list`

## The popups

They live in the body now, like the context menu, and are measured and clamped when opened. The
context menu already had to do this for the same reason; doing it the same way means there is one
answer to "where do floating bits go" rather than two that fail differently.

Measuring is no longer deferred a frame either: the element is in the body, so it has a size as soon
as it is shown, and the frame's delay was only ever covering for the fact that it was not.

## The warning

Ordered onto the name's line, flexible and truncated, so a hop is one line. The hover now changes the
underline and brightens, because a thing you can click has to look like one and a dotted underline on
its own was not saying it.

## The list

The docked panel is the box and the hop list is what scrolls inside it. With the panel scrolling
instead, the list sized to its own content and stopped short of the bottom of the dock.

## Verified

By reasoning and the suite, not by screenshot: the demo's map had not finished loading in the runs
taken for this, and the harness cannot click a popup open. The three changes are each a rule the
browser applies, not a behaviour that needs driving.
