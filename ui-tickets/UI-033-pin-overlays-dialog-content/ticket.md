# UI-033 &mdash; The always-on-top pin overlays dialog content

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `ontop_pin`, `dialog_viewport_ext` |
| **Found by** | GAP-001 |

## Symptom

In every dialog that routes through `dialog_viewport`, roughly 22 of them, the always-on-top pin is
drawn over the dialog's own content. Measured:

```
dialog_jump_bridges:  pin [[401.0 6.0]-[434.0 33.0]]  over  Label "Imperium stargates" [[295.5 13.5]-[408.0 28.5]]
dialog_coalitions:    pin [[481.0 6.0]-[514.0 33.0]]  over  the header paragraph
dialog_battle_filter: pin [[541.0 6.0]-[574.0 33.0]]  over  the header paragraph
```

In `dialog_jump_bridges` the pin sits **on a clickable hyperlink**, so the link cannot be clicked
where it is covered. That is the worst case; elsewhere it covers header words.

## Cause

`ontop_pin` is a floating `egui::Area` anchored `RIGHT_TOP` at `Order::Foreground`. It reserves no
space, so the dialog body lays out underneath it.

This is the same defect UI-020 fixed for the jabber popout by moving the pin into the tab-bar row.

## Correcting UI-020

UI-020's review examined all four `ontop_pin` callers and left three alone, recording
`dialog_viewport_ext` as "unchanged, and it is the shared body of ~18 dialogs. None has a tab row to
host a pin". That was accepted at the time. It was wrong: having nowhere convenient to put the pin is
not evidence that the floating `Area` is harmless.

None of these dialogs had a harness scene until GAP-001, which is why it went unseen.

## Direction

UI-020's fix put the pin in an existing row. These dialogs have no such row, so the options differ:

- Reserve space at the top-right of the dialog body, so the content lays out clear of it.
- Give `dialog_viewport_ext` a small title strip that owns the pin, accepting the height cost on
  every dialog.
- Inset the dialog's `CentralPanel` by the pin's width where the pin is shown.

Whatever is chosen must not reintroduce the UI-020 problem in the other direction: the pin must stay
visible and clickable.

## How to verify

The eight GAP-001 dialog scenes cover this directly, and three of them currently show the overlap.
Note `uitest_layout` does **not** catch it today, see GAP-010; fixing that gap would give this ticket
a hard gate.
