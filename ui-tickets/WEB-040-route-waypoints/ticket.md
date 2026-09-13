# WEB-040 &mdash; A second route drag starts over; the start dialog shrinks as you type

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/map.js`, `assets/dialogs.js`, `app.rs` |
| **Reported by** | user |

## Symptoms

1. "Routing features: If another line is drawn from the destination system, it is converted into a
   waypoint instead and the new target system will be the destination."
2. "The Start a DM/Join a Room window should have a stable height. It shrinks when too few matches are
   found"

## Cause

1. WEB-039 held one pair. A drag off the destination threw the first route away instead of extending
   it.

2. Both the desktop list and the browser one were capped rather than fixed, so they collapsed as the
   search narrowed.

## How to verify

Drag A to B, then B to C. The route has to be A to B to C with B drawn once. A fix would be WRONG if
the waypoint appeared twice or added a jump.
