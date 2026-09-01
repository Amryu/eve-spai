# UI-047 A server force-join into a new room surfaces nowhere

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `jabber_reconcile` |
| **Reported by** | user, amending UI-046 |

## Symptom

The server can put the user into a room they never asked for: a bookmark, an invite, or an
alliance mandating a channel. UI-046 stopped that from opening a tab, on the rule that the tab
bar holds what the user opened. The room appears as a new row in the Channels list and nothing
else happens.

That is too quiet. A channel someone was put into is usually one they are expected to be
reading, and a new sidebar row among the others is not a notification.

## Wanted

A force-join opens the tab exactly **once**, the first time the room is seen. After that it is
an ordinary room: closable, hideable, and not reopened on the next start or the next reconnect.

## Notes

- "First time" already has a marker. `jabber_reconcile` copies any room in `f.rooms` that is not
  yet in `settings.jabber_rooms` into it, and that branch fires exactly once per room, because
  every subsequent frame finds it already there.
- Rooms the user joins by hand are pushed into `jabber_rooms` by the join dialog and the
  Channels-list Join button before the join lands, so they never reach that branch. It is
  genuinely server-driven joins only.
- The pinned Rescue Mode rooms (UI-045) are added to `jabber_rooms` by the healing step at the
  top of the same function, before this branch runs, so pinning must not start opening tabs.
- The room may be on `jabber_closed_rooms` from a previous life. The entry has to be cleared or
  the tab is opened and then pruned on the next frame by the `want` filter.

## How to verify

`cargo test --bin eve-spai jabber_force_join`. The fix is WRONG if the tab reopens on a second
reconcile, on a reconnect, or after a restart; if a hand-joined room opens twice; or if enabling
Rescue Mode starts opening delve911 and skirmish_commanders tabs.
