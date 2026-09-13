# WEB-038 &mdash; A theme change takes the web server down; the range tint drops the moment you move the pointer

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `web/server.rs`, `web/state.rs`, `assets/map.js`, `app.rs` |
| **Reported by** | user |

## Symptoms

1. "Changing the theme in the app seems to crash the web server (or maybe changing any setting at
   all?)"
2. "Web map: selecting a system should keep the jump range color indicators visible (hovering another
   system still overrides it, just like the app)"

## Cause

1. `sync_web_server` hashed the theme's three colours into the key that decides whether to restart
   the listener, because the sheet was served from `Config`, which only changes by restarting. So
   moving a colour slider dropped the `Handle` and immediately rebound the port.

   Dropping the handle tells the workers to stop and unblocks the accept, but the socket is only
   closed when the last worker lets go of its `Arc<Server>`, and they wake on a 500ms timeout. The
   rebind therefore raced a socket that was still open, failed, and `sync_web_server` recorded the
   failure and left the feature off. From the browser that is the web view crashing on a colour
   change.

2. The tint followed `hovered`, and the hash-driven selection was writing into the same variable, so
   moving the pointer anywhere else cleared it.

## How to verify

Move the accent slider with a phone connected. A fix would be WRONG if it only made the restart
succeed: restarting a listener because a colour changed is the bug.
