# WEB-012 &mdash; The feature cannot be turned on

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | the settings view region of `app.rs` |
| **Reported by** | user, remote web view |

## Gap

Everything from WEB-001 to WEB-011 is reachable only by editing the settings blob by hand. There is no
switch, no way to read the pairing link, and a failed bind is invisible.

## Deliverable

A section in `settings_view` (`app.rs:16853`), following the local idiom
(`changed |= ui.checkbox(..).on_hover_text(..).changed()`):

- Enable checkbox, port, and a LAN / loopback-only toggle.
- The reachable URL including the token, with a copy button.
- Regenerate link, which clears the token and drops every live client with `event: bye`.
- Open in browser, via `open::that`, as `start_login` already does.
- Live client count.
- The bind-failure warning in `standing::WARNING`, matching how `alerts_view` warns when alerts are
  off.
- A plain sentence: LAN only, no TLS, do not port-forward it.

## Notes

Toggling `enabled` or changing the port restarts the listener; it must not need an app restart.

Confirm a phosphor icon exists before using it or it renders as tofu. No small font sizes on content
text.

A QR of the paired URL is the mobile-friendly way to hand the token to a phone, and it needs one new
pure-Rust dependency. It is deliberately not in this ticket: land the link first, then decide on the
dependency in a follow-up.

## How to verify

- A `uitest` scene for the settings section, through the normal `ui-tickets` flow, plus the census to
  confirm the new controls are real hit targets and not a chrome-baseline scene.
- Toggle enabled off and on with the app running and confirm the page goes away and comes back.
- Set the port to one already in use and confirm the warning appears and the app keeps running.
- A fix would be WRONG if it printed the token into a log line, or if a bind failure could panic.
