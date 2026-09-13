# WEB-015 &mdash; The map drew wormhole space, rebuilt itself on every push, and the dialogs could not be closed

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `web/map.rs`, `assets/map.js`, `assets/app.js`, `assets/layout.*`, `assets/dialogs.css` |
| **Reported by** | user, running the installed build |

## Symptom

Five things, reported together after the first real use:

1. Auto, Columns and Grid show every pane, with no way to switch one off.
2. The map does not fill the space it is given.
3. A pane in grid mode makes the **whole page** scroll instead of scrolling inside its box.
4. The map shows systems that are not on the star map.
5. Dialogs cannot be closed, and the whole page feels laggy.

## Measured

From the running instance, against the real SDE:

| | |
|---|---|
| Nodes in `/api/map/geometry` | 8490 |
| of which wormhole space | 2604 |
| of which abyssal or void | 401 |
| with no stargate at all | 3222 |
| Payload | 656 KB |
| Nodes the desktop map would draw | 5484 |

## Cause

**Systems that should not be there.** `web::map::build` took every row of `sde_systems`. The desktop
map filters twice before drawing: a system with no connections, and a system whose region name
contains a digit, which is how J-space (`A-R00001`), abyssal and the Jove regions are named and
normal space is not. The web build did neither.

**The lag.** Two compounding mistakes. `render()` redrew **every** pane on every SSE push, and the
map's renderer rebuilt its entire `innerHTML` on every call: 5000+ circles plus a 7000-segment path,
several times a second. It also rebuilt on every pan and every zoom.

**The dialogs.** `close()` sets `hidden`, but `.modal { display: flex }` is an author rule of the
same specificity as the UA's `[hidden] { display: none }`, and an author rule wins. The dialog was
never hidden by anything.

**The scrolling.** `#panes` had `overflow: auto` and panes were bounded by `max-height: calc(100vh -
7rem)`, a guess at the chrome height that is wrong with two grid rows. So the container scrolled
rather than the pane.

## Notes

The mobile controls were called out separately: the data display reads fine narrow, the controls do
not.

## How to verify

`app/src/uitest/webshot.sh`, and the running app after a reinstall. The map must show only k-space,
fill its pane, and pan without the page moving. A pane must scroll inside its own box in grid mode.
A dialog must close on its button, its backdrop and Escape. A fix would be WRONG if it made the map
fast by drawing less of the real map, or if it stopped the page scrolling by clipping content that
has nowhere else to go.
