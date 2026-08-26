# UI-021 &mdash; Composer should grow to 10 rows, and the Send button should go

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | the composer row in `jabber_window_body` |
| **Reported by** | user |
| **Blocked on** | GAP-004 for visual verification |

## Symptom

Two requests against the same widget, at `app.rs:3985-4017`:

1. The message field is fixed at `desired_rows(2)` inside a `ScrollArea` capped at `row_h * 8.0`. It
   should **grow as the draft grows, up to 10 rows, then scroll**.
2. The `Send` button is unnecessary; Enter already sends (`return_key(shift_enter)` makes Shift+Enter
   insert a newline, and a plain Enter is caught as `send`).

## Notes

- Removing the button frees the `- 60.0` the field currently subtracts from `available_width`.
- Growth should track what the user can actually see, so wrapped long lines count, not just newline
  count. The galley height is the honest measure; `desired_rows` alone counts logical rows.
- The empty state should still be small; do not start at 10 rows.
- Enter-to-send must keep working after the button is gone, and Shift+Enter must still insert a
  newline. Both need a test.

## How to verify

Needs a jabber popout scene (GAP-004). Render the composer empty, at ~3 rows, and at more than 10
rows, and confirm it grows then scrolls. Drive Enter and Shift+Enter through the harness.
