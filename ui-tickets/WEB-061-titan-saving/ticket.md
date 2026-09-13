# WEB-061 &mdash; A titan route does not say whether it is worth taking

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/dialogs.*`, `app.rs` |
| **Reported by** | user |

## Asks

1. "Titan route should also state how many jumps are being saved over not using the titan. If it is 2
   jumps or lower warn that a direct route may be better."
2. "Warning messages: leave away the '1 hour' in the route list. It is implied"

## Gap

The search already knew the gate route's length, since it compares against it to decide whether an
option is worth offering at all, and then threw the number away. The user was left to work out whether
lighting a cyno for this was worth it.
