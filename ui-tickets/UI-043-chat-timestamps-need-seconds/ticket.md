# UI-043 Chat timestamps are minute-resolution

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `eve_time_label` (app.rs), used by the Jabber history and `rescue_chat_ui` |
| **Reported by** | user |

## Symptom

Every chat message in the Jabber window and the Rescue window is stamped to the minute.
Two reports thirty seconds apart read as the same instant.

That is the wrong resolution for what these windows are for. A hostile report, a tackle
call and a rescue request are ordered and acted on in seconds, and the log is what gets
read back afterwards to work out what happened when.

## Measured

`eve_time_label` at `app/src/app.rs:19442`:

| Case | Format | Renders as |
|---|---|---|
| Same day | `EVE %H:%M` | `EVE 12:02` |
| Older | `EVE %Y/%m/%d %H:%M` | `EVE 2026/08/30 12:02` |

Both call sites render it identically, `.weak().size(9.5)`:

- `app.rs:4170`, the Jabber history
- `app.rs:22260`, the Rescue window chat

The underlying data is already second-accurate: `ChatMsg.time` is a unix timestamp, set
from the stanza's `<delay/>` stamp or `Utc::now().timestamp()` at arrival. Only the
formatting throws the seconds away.

## Notes

- One helper serves both windows, so this is a one-line format change plus its tests. The
  two windows must not drift apart.
- Consecutive messages from one sender within five minutes share a header
  (`rescue_grouped`, and the same rule in the Jabber history), so only the first message of
  a group carries a timestamp at all. Second-accuracy therefore lands on group heads only.
  Whether every message should carry its own time is a separate question, not assumed here.
- The label is `.size(9.5)`, below the app's body size. Adding three characters makes a
  small string smaller-looking still. Not changed here, see UI-044.

## How to verify

`cargo test --bin eve-spai eve_time_label` plus the Jabber and Rescue screenshots. The fix
is WRONG if the two windows end up with different formats, if the older-than-today form
loses its date, or if the row height changes and the history re-flows.
