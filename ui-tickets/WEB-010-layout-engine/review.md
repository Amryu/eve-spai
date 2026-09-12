# WEB-010 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-010-layout`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 640, unchanged: this ticket is CSS and DOM, which the Rust suite cannot see |
| **Follow-ups** | none |

## What changed

Four modes. `auto` is a media query rather than a stored choice, so rotating a tablet does the right
thing without anyone having picked anything; `tabs`, `columns` and `grid` are explicit. Mode, pane
order and the active tab live in `localStorage`, per device. `Settings.web.default_layout` seeds a
first visit and nothing more: a phone and a desktop browser want different layouts, and the app
pushing one to both would be the app fighting the user.

**Swipe is the browser's.** `tabs` is a scroll-snap strip, `scroll-snap-type: x mandatory` with each
pane at `min-width: 100%`. There is not one touch handler in the file. Hand-rolled
`touchstart`/`touchmove`/`touchend` gets momentum, over-scroll and interrupted gestures wrong and
fights iOS; all `layout.js` does is read back, on a debounce, which pane the strip landed on.

**Order is applied with `style.order`, not by moving nodes.** A re-render therefore never rebuilds
the DOM, which matters because the intel pane holds a focused search field.

Drag to reorder: straight drag on a pointer device, long-press then drag on touch, both ending in the
same `move`.

## A defect the screenshot caught

The mode buttons rendered *between* `Intel` and `Alerts`. The tab chips are ordered with
`style.order`, an unset `order` is `0`, and the mode bar had none, so it tied with the first tab and
document order broke the tie. It is `order: 99` now.

The intel pane also had no heading, which only showed once panes became boxes in column mode: every
other pane draws an `h2` and it drew none. It has one, hidden in tabs mode where the tab chip is
already the heading.

## Verified

`after/web-1440.png`: four columns, each pane a bordered box with its own scroll.
`after/web-390.png`: `auto` resolves to tabs, one pane at a time, the Intel chip marked selected, the
mode switcher sitting after the tabs where it belongs.
