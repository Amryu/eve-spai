# WEB-020 &mdash; Map link styling, labels, hover, and the Mumble link that opened a browser

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `app.rs` map drawing, `web/map.rs`, `assets/map.js`, `assets/{app,layout}.*` |
| **Reported by** | user |

## Symptom

Map, in both the app and the browser:

- Gates are all the same line, so nothing says where a constellation or a region boundary runs.
  Inter-constellation should be dotted, inter-region dashed.
- Jump bridges should be green arches, not straight lines, and should be overridden where a route
  uses them.
- The route the app draws is not drawn in the browser at all.
- Jump bridge connections "are being culled way too aggressively".
- System names show when zoomed out far, and "only render for some regions, even if I am zoomed out
  very far and can see many more". Region names should show instead, as in the app.
- "The drag cursor is being overprioritized, I can't see when I am properly hovering a system."

Elsewhere:

- The burger button "is still disconnected from the options window that opens", and on the desktop
  belongs at the far right.
- The Join button works but "should use the mumble: link if available like it does in the app to
  avoid opening a browser page".

## Cause

**Culling.** A segment was dropped when *both* endpoints were off screen. That is right for a gate
between neighbours and wrong for anything long: a bridge spanning the viewport has both ends outside
it.

**Labels.** The cap was `if (++n > 300) break` over `geo.nodes`, which is sorted by system id. So the
first 300 systems by id got labels and everything after them did not, which reads as whole regions
missing. The zoom threshold was on the dot radius, which saturates at its clamp, so it stopped
discriminating exactly when it mattered.

**Cursor.** `.starmap { cursor: grab }` plus `:active { grabbing }` in the stylesheet always won,
because nothing else set one.

**The menu.** The panel was rendered inside `#tabs` and positioned against `#bar`, so the button and
its panel had no relationship to each other.

**Mumble.** `join_comms` called `open::that` on the ping's link. A ping's comms link is usually a
`gnf.lt` page that redirects; the app resolves that page to the real `mumble://` URL first with
`open_mumble`, which is why the app lands in Mumble and the web view landed in a browser.

## How to verify

In both maps: a constellation boundary reads dotted, a region boundary dashed, bridges arch in green,
and a route overrides what it runs along. Zoom out and see region names, zoom in and see system
names, everywhere, not in patches. Hover a system and see it. A fix would be WRONG if it made
bridges visible by not culling anything.
