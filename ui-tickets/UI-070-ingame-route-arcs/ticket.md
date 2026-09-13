# UI-070 &mdash; The in-game route draws jump bridges as straight lines

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `app.rs` map, in-game destination route |
| **Reported by** | user |

## Symptom

"The in-game route being set still shows the animated path in the in-app map. It draws the bridges the
old way, make sure it uses the arc"

## Cause

The route drawn for the ESI destination walks its legs through `leg_kind` for the colour and then
hands every one of them to `dashed_flow`, which is a straight line. UI-052 and the tickets around it
gave arcs to the bridge layer, the travel route and the picked route, and this one was missed because
it is a fourth drawing of the same idea.

## How to verify

Set a destination whose route uses an ansiblex. The bridged hop has to arc, in the route's own colour.
