# WEB-023 &mdash; Map markers are squares, not the app's icons

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js`, `web/icons.rs` |
| **Reported by** | user |

## Symptom

"Upgrades, jove observatories, whs, etc. do not show the proper icons like in-app. I just see some
squares."

## Cause

The canvas renderer drew shapes: a filled rect for an upgrade, a diamond for a mining one, an empty
square for a Jove observatory, a cross for a cyno generator. The page already serves the app's own
Phosphor font and the name-to-codepoint map, and the map was not using either.

Drawing a glyph on a canvas also needs the webfont to have **loaded**: canvas falls back silently
when asked for a font it does not have yet, which is itself a way to draw a box.

## Deliverable

The app's own glyphs: a skull for a ratting upgrade, a broadcast dish for exploration, a gear for
anything else, a cell tower for a Jove observatory, a spiral at each end of a scanned wormhole, a
campfire on a gate camp, a crosshair for a cyno generator. A mining upgrade draws the ore's own EVE
type icon, as the app does.

## How to verify

The demo map. A fix would be WRONG if it drew glyphs before the font was ready, which puts the boxes
back for the first paint of every load.
