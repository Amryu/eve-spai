# WEB-029 &mdash; No jabber in the web view

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/jabber.rs`, `assets/panes-jabber.{js,css}`, `ipc.rs`, `app.rs` |
| **Reported by** | user |

## Ask

"Add a new view for jabber, which is showing the 'Convos' list and the chat that has been selected
there. No tabs are available here, the selection is done via the convos list. It should also be
possible to start new convos, the exact same way as in the app."

## Gap

The phone could read intel, alerts, pings and the map, and nothing of the conversation those were
being discussed in. Fleet chat was the one thing that still needed the desk.

## Acceptance

The same Convos list the app shows, in the same order, with the same unread counts and mention
marks; a conversation selected from it; and the start dialogs reachable from the list, resolving a
name the way the desktop dialog resolves it.
