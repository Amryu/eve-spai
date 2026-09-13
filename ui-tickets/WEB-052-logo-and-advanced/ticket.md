# WEB-052 &mdash; No logo, a long pane name, and no way to bind an interface or skip pairing

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `settings.rs`, `web/server.rs`, `app.rs`, `assets/index.html` |
| **Reported by** | user |

## Asks

1. "Shorten the panel name to 'Fleets' from 'Fleet pings'"
2. "Add the application logo to the top left of the page (before the 'EVE SPAI' label)"
3. "Add 'advanced' options to the web server, allowing the user to set the binding address himself and
   to skip the need of pairing. First time accessing these, show a warning message that this might
   potentially expose sensitive data to the public and that he has to make sure what he is doing.
   (Especially private jabber convos and opsec channels). Double-check that this is never made the
   default."

## Acceptance

For 3, the important half is the last sentence.
