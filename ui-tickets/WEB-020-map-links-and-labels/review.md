# WEB-020 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-020-map-hover`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 678 to 680 |
| **Follow-ups** | none |

## Link styling, in both maps

Gates now say which boundary they cross: solid inside a constellation, dotted across one, dashed out
of the region. Jump bridges are green arches rather than straight lines, because a bridge and a gate
between the same pair were otherwise the same stroke in a different colour, and colour alone does not
survive a busy map.

A route leg that runs along a bridge takes the same arch in the route colour, so it overrides the
bridge instead of crossing it with a differently-shaped line.

The app and the page sample the same quadratic with the same bow constant. The geometry payload
carries a style per edge (0, 1, 2) rather than the page re-deriving boundaries it has no region data
for.

The route itself is now in the snapshot as an ordered list of system ids and nothing else: the page
already knows which pairs are gates and which are bridges from the geometry, so it can tell a solid
leg from an arched one from a dashed one without being told.

## Two culling bugs, opposite directions

**Segments were culled too hard.** A segment was dropped when both endpoints were off screen, which
is correct for a gate between neighbours and wrong for anything that spans the viewport. It is a
bounding-box test against the viewport now, which keeps anything that could possibly be visible.

**Labels were culled arbitrarily.** `if (++n > 300) break` walked `geo.nodes`, which is sorted by
system id, so the first 300 systems by id were labelled and the rest were not. That is why whole
regions looked skipped: the cap had nothing to do with what was on screen.

The zoom test was also wrong in a way that hid it. It keyed on the dot radius, which is clamped, so
past a certain zoom it stopped changing and the threshold stopped meaning anything. It keys on the
visible span now, and zoomed out the map draws **region names** at each region's centroid, which is
what the app shows and what is legible at that scale. With a real threshold the visible set is small
enough not to need a cap at all.

## Hover

A stylesheet `cursor` wins over nothing, so a hovered system looked exactly like empty space. The
pointer handler sets the cursor as it moves, and hovering draws a ring and a readout with the
system's name, security and ADM.

A cursor alone would not have been enough anyway: a touch screen has no cursor, so a tap leaves the
system under the finger highlighted, which is the only hover feedback a phone can give. A
`#system/<id>` link highlights its system too, so a link someone sends points at something visible.

## The menu

It was rendered inside `#tabs` and positioned against `#bar`, so the button and the panel were
unrelated by construction. The menu owns its own button in its own positioned container now, and sits
at the far right of the bar, which is where a menu button belongs. On a phone the status word was
costing a whole column and is gone: the tab counts and the feed already say whether anything is
arriving.

## Mumble

`join_comms` opened the ping's link directly. A comms link is usually a `gnf.lt` page that redirects,
so that opened a browser. It goes through `open_mumble` now, the same path the app's own Join button
uses, which fetches the page, extracts the real `mumble://` URL and opens that, falling back to the
browser only when it cannot resolve.

One line, and it is the difference between the feature working and appearing to work.
