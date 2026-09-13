# WEB-015 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-015-map-perf-and-panes`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed, all five, plus the mobile controls pass |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 659 to 664 |
| **Follow-ups** | none |

## The map was drawing a third of the game that has no gates

`web::map::on_the_map` now applies the desktop's own two rules rather than inventing a third answer:
nothing connects to it, or its region name contains a digit. That is 2604 wormhole systems, 401
abyssal ones and 3222 with no stargate, gone. The filter runs **before** the index is built, so every
edge still indexes the list it was built against; `edges_still_index_the_filtered_list` is there
because doing it after would silently shift every edge by one.

## The lag was two things, and both were mine

**Every pane redrew on every push.** The server already says which panes changed, and `merge()` threw
that away. It returns a dirty set now, and `render(dirty)` honours it. A meta change still redraws
everything, because meta carries compact mode and the theme.

**The map rebuilt its whole DOM on every call**, including on pan and on zoom. It is three layers now:

| layer | rebuilt |
|---|---|
| base: the edge path and one circle per system | once, when geometry arrives |
| live: intel and character markers | when the map pane's `rev` changes |
| labels | debounced, only what is on screen, only when zoomed in enough to read |

Pan and zoom set one attribute and touch no DOM at all. The old code also re-rendered on a 120ms
debounce after every zoom, which is what made the gesture itself feel bad.

## The dialogs

One line. `.modal[hidden] { display: none; }`. `[hidden]` is a UA rule of the same specificity as
`.modal`, author rules win, and `display: flex` kept the dialog on screen however many times the
close button was pressed. The close target also went from 2.2rem to 2.75rem, which is the difference
between hitting it with a thumb and not.

## Scrolling

`html, body { overflow: hidden }`, `#panes` clips, and each pane is a flex column whose body scrolls.
Grid cells are `minmax(0, 1fr)` and panes stretch to fill them, so a pane is bounded by its cell
rather than by a guess at the chrome height. That is also what lets the map fill its pane: it is
`flex: 1` inside the pane instead of `min(60vh, 520px)`, and a `ResizeObserver` refits it when the
pane changes shape.

## Pane visibility, and the mobile controls

The mode chips were four buttons sitting in the bar competing with the tabs. They are now a layout
menu behind one `≡` button: modes on one row, pane on/off on the next, a dropdown on a desktop and a
bottom sheet on a phone, same markup. A pane switched off leaves its tab chip in the bar, dimmed and
dashed, because that chip is also how it comes back. It refuses to switch the last pane off.

On a phone the bar is a two-row grid, the tabs scroll sideways rather than wrapping into three rows,
every control is at least 2.4rem, and the arm button drops its sentence for a glyph.

## One more thing the screenshots caught

The icon map was fetched after first paint, so the page painted once without glyphs and again with
them. It is inlined into the document now, like the snapshot. That removes a request, removes the
flicker, and makes screenshots deterministic instead of racing the fetch.

`#layout` opens the menu on load, for the same reason the dialogs take deep links: the harness cannot
click, so without it the menu is the one control no screenshot can show.

## Verified

`after/menu-phone.png`: the bottom sheet with both rows at thumb size, over a feed scrolling inside
its own pane. `after/menu-desktop.png`: the same menu as a dropdown.
