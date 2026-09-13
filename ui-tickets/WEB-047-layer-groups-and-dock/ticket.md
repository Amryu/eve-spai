# WEB-047 &mdash; Eleven layer buttons in a row, a floating route window, and half-done avoidance

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.{js,css}`, `assets/dialogs.{js,css}`, `snapshot.rs`, `app.rs` |
| **Reported by** | user |

## Asks

1. "There are quite a few filter buttons on the map at this point. Categorize them and put them into
   some popup menus by clicking the specific button. The popup should not immediately close after
   pressing one of the options, only after clicking elsewhere, clicking another button or the same
   button again"
2. "The highlight for each waypoint system in the list sucks. Highlight the system with a different
   background color perhaps."
3. "The option to avoid should also show in the route window"
4. "'Stop avoiding always' should only show when it is actually being avoided. When in routing mode,
   highlight avoided systems"
5. "The jump route window should instead dock at the bottom or the right of the map panel (depending
   on the view). The jump/gate list should be scrollable."

## Cause

1. Eleven controls behind one Layers button, all in one flat grid, with nothing saying which of them
   belonged together.
2. An outline on the chip, which reads as a focus ring rather than as "you chose this one".
3. Avoidance was reachable only from the map's context menu, which means knowing where the system is
   before you can say you would rather not go there.
4. The page had no idea what was on the permanent lists, so it offered both options always.
5. The route window floated over the map it was describing.
