# WEB-035 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-035-own-windows`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693, unchanged |
| **Follow-ups** | none |

## The store

`sync_web_server` opens one when it starts the server. A second connection rather than the app's:
`Store` owns a rusqlite `Connection` and cannot be shared across threads, and SQLite is happy with a
second reader on the same file. Opened once, and only when the feature is switched on, so a user who
never turns it on never pays for it.

## One window per kind

`shells` is keyed by kind, so a system, a ship and a pilot are three windows. They cascade 18px from
the map's corner so three open at once are all reachable, and whichever was opened last comes to the
front. Escape closes the one it belongs to rather than all of them.

## The X

On each pane, next to the handle, and it drops the pane's span on the way out. A pane that comes back
should come back as one cell: keeping the span would mean switching a pane on and having it arrive
two cells wide, pushing something else out to pay for a size nobody asked for in this layout.

## Verified

The demo cannot show this one: its profile has no SDE, so a fixed endpoint and a broken one both
answer "not in the static data". Checked against the running app instead:

```
$ curl /api/ship/11993
{"id":11993,"name":"Cerberus","group":"Heavy Assault Cruiser","shield_hp":2000.0,...
```

`after/pane-close-buttons.png` is the grid with the close button on each pane.
