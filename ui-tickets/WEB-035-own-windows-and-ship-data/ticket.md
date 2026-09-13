# WEB-035 &mdash; Ship dialog has no data and steals the map's window; no way to close a pane from itself

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/dialogs.js`, `assets/layout.js`, `app.rs` |
| **Reported by** | user |

## Symptoms

1. "Clicking ships in the intel cards just shows 'no static data' and also re-uses the maps floating
   window. It should use it's own window and fix the missing data"
2. "Add an 'X' to each panel, which will disable it (and also reset it's size)"

## Cause

1. Two separate faults behind one click.

   `DetailState.store` was declared in WEB-008 and never assigned, so `/api/ship/{id}` always took the
   `None` branch and answered "not in the static data". The field existed, which is what made it look
   done.

   The dialogs shared one `.float` node, so opening a ship replaced whatever system was being read,
   on top of the map it was read from.

2. Pane visibility was only reachable through the layout menu, two taps away from the pane it is
   about.

## How to verify

A ship from an intel card, on the live app rather than the demo: the fixture profile has no SDE, so
the demo cannot tell a fixed endpoint from a broken one.
