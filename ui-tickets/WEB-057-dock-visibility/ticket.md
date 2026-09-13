# WEB-057 &mdash; A dialog opened with the map pane off renders into nothing

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `assets/dialogs.js` |
| **Reported by** | user |

## Symptoms

1. "web alerts: Clicking a ship no longer opens a dialog with info"
2. "Shorten the ', last hour' to '(1h)' in the system docked window on the web map"

## Cause

WEB-048 docks every window into the map pane when there is one. It checked that the pane *existed*,
not that it was showing, so with the map switched off the window was created inside an element with
`display: none`. The click worked, the fetch worked, the dialog rendered, and none of it was on
screen.

## How to verify

A ship from an intel card with the map pane switched off. A fix would be WRONG if it only checked
`hidden`: a pane can also be off screen because its whole layout is.
