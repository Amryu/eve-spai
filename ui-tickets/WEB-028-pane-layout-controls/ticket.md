# WEB-028 &mdash; No way to rearrange or resize panes, and no budget on how many there are

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/layout.{js,css}`, `icons.rs` |
| **Reported by** | user |

## Ask

"Allow rearranging views in grid/tab mode by drag and dropping. There should be a grabber in the top
right of each panel and proper highlighting when drag and dropping as to what will happen. There
should also be a button to expand to 2 columns or 2 row span. The grid can only ever be 2x2, if no
space is available, the view will be disabled. (tab view only accepts up to 4 columns too)"

Followed by: "Make sure customizations are persisted in the browser".

## Gap

Order could only be changed by dragging the tab buttons, which is the one control that is not visible
in grid or columns mode, where rearranging actually means something. A pane had no way to claim more
of the grid. Nothing bounded how many panes could be on at once, which the fifth pane (WEB-029) makes
a real problem rather than a theoretical one.

## Acceptance

A handle and a span button on each pane in grid mode; a drop that says what it will do before it
happens; four cells spent and no more, with the overflow switched off; all of it in `localStorage`
with the rest of the layout.
