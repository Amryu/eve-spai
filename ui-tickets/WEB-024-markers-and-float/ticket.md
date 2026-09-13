# WEB-024 &mdash; Map markers are tiny and stack on each other; the system window covers the filters

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js`, `assets/dialogs.js` |
| **Reported by** | user |

## Symptom

- "Icons are tiny and not properly spaced" on the web map.
- "The system names could be a bit bigger too."
- "The floating window on the map still starts too high, covering the filter options."

## Cause

**Markers.** Each layer drew its own glyph at its own fixed offset above the system dot, and three of
them used the same offset. A system with a camp and a wormhole drew both in the same place. Upgrades
laid theirs out from the left edge of the dot rather than centred, so a system with three ran off to
the right across its neighbours.

**Sizes.** 12 and 13 pixels for markers, 11 for names.

**The window.** It aligned to the canvas's own top. On a narrow pane the layer panel is an **overlay**
sitting over the top of the canvas, so a window aligned to the canvas lands on the filters.

## How to verify

A system carrying several markers draws them in one row, centred, clear of the dot. Opening a system
while the layer panel is open puts the window below the panel. A fix would be WRONG if it placed the
window from an off-screen canvas rect: in tabs mode the map is scrolled away and its rect is nowhere
near the viewport.
