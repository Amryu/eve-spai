# WEB-031 &mdash; A swap moves the pane and leaves the size behind

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `assets/layout.js` |
| **Reported by** | user |

## Symptom

"Swapping views should also swap the size."

## Cause

`span` is keyed by pane name, and WEB-028's `swap` only exchanged positions in the order. A pane
dropped onto the wide cell arrived narrow and dragged the other one's width off to wherever it went,
so a swap rearranged the grid as well as the panes in it.

## How to verify

Widen one pane, drop another onto it. The grid should look the same afterwards, with the two panes
the other way round.
