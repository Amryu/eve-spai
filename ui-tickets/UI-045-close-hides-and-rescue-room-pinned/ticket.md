# UI-045 Closing a room tab leaves the channel, and can silently kill Rescue Mode

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `close_jabber_tab`, `jabber_close_room_dialog`, `jabber_forget`, `jabber_rooms_to_join` |
| **Reported by** | user, diagnosed from their live profile |

## Symptom

Reported as "chats are still reopening after restarts", after UI-039 shipped. The rooms are
not reopening. The user cannot close them in the first place.

## Measured

From the reporting user's live profile, read-only, counts only:

| Key | Value |
|---|---|
| `jabber_rooms` | 5 |
| `jabber_closed_rooms` | **0** |
| `jabber_close_room_leaves` | **true** |
| `jabber_closed_dms` | 17, covering 14 of 15 stored DM conversations |
| rooms rejoined by the server this session | 0 |

UI-039 works: nothing is being resurrected. The hide path is simply unreachable.
`close_jabber_tab` only raises the prompt when `jabber_close_room_leaves` is `None`. Once it
is `Some(true)`, the tab X leaves the channel with no prompt and nothing ever reaches
`jabber_closed_rooms`, so `jabber_reconcile` opens a tab for all five joined rooms on every
start.

The user answered that prompt once, long before the answer was permanent, and the only way
back is a three-way selector buried in the Jabber alerts window.

## The dangerous case

With `fc_rescue_enabled`, closing the delve911 tab leaves the room, and Rescue Mode dies
silently. `ingest_delve911_jabber` (`app.rs:4903`) reads `st.chats[delve911_jid]`, and since
UI-039 a left room's messages are dropped in `note_room_seen` before `push_msg` stores them.
The parser goes quiet, the ping feed stops, and `play_delve911_alert` never fires. No error
is raised anywhere, because leaving is what the user appeared to ask for.

That is a capital-rescue tool failing closed, from one click on an X.

## Wanted

1. The tab X always hides. Closing a tab is not a destructive act and must not ask.
2. Leaving is the sidebar remove button (UI-041) and only that. `jabber_close_room_leaves`
   and its prompt go away.
3. While Rescue Mode is on, the configured delve911 room is pinned: joined on every connect,
   and not removable from the sidebar.

## How to verify

`cargo test --bin eve-spai jabber_close` and `jabber_rescue_room`. The fix is WRONG if the X
can still leave a room, if a pinned delve911 can be left or forgotten by any path, if pinning
happens in a build without `fc-rescue` or with Rescue Mode off, or if the delve911 tab can no
longer be hidden. Hiding it is safe and must keep working: the room stays joined and the
parser keeps reading it.
