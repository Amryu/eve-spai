# WEB-058 &mdash; A waypoint can be added from the row menu but not taken back

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `assets/dialogs.js`, `app.rs` |
| **Reported by** | user |

## Ask

"Add an option to remove waypoints in route planning from the '...' button."

## Gap

UI-056 put adding a waypoint on the row menu and left removing one on the map's context menu, so the
two halves of the same decision lived in different places.
