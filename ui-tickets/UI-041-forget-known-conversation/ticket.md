# UI-041 No way to forget a remembered room or private chat

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `jabber_channels_list_ui`, the Directory pane rows in `jabber_sidebar_ui`, `jabber_frame` |
| **Reported by** | user |

## Symptom

The sidebar remembers every room and private chat the user has ever had a message in, and
offers no way to remove one.

- **Directory pane.** Rows are grouped by XMPP roster group; anything with stored chat
  history and no roster entry falls into the catch-all group **Other**. Rooms land there
  too, because room messages are stored under the room JID in `chats`. The group grows
  monotonically and the only per-row control is the contacts star.
- **Channels pane.** Every known room, including ones the user was kicked from months ago
  (struck through, history only). No per-row control at all.

Remembering is the right default. The user's words: "Remembering the rooms the user has
used before is good, but he should also have a button to remove them here."

## Measured

`Convo` rows in the Directory come from two sources in `jabber_frame`:

| Source | Group | Removable today |
|---|---|---|
| `st.roster` | the roster group | server-side, not ours to remove |
| `st.chats` keys | `"Other"` | no |

`ChannelRow` in the Channels pane comes from `settings.jabber_rooms` + `st.rooms` +
`st.rooms_inaccessible`. UI-039 made a deliberate leave drop a room from that set, but a
leave needs an open tab to reach the close X, and it does nothing for a DM or for a room
the user is no longer in.

## Cause

Nothing removes a JID from `st.chats`, and no persisted set excludes one from the lists
built off it. `jabber_left_rooms` (UI-039) is close but wrong for this: it means "do not
rejoin", it is rooms-only, and a left room is still offered in the browse results on
purpose, so the user can rejoin it.

## Notes

- History is kept. The user chose this explicitly: the store rows survive, so rejoining
  restores the backlog. That means hiding cannot work by deleting from `st.chats`, since
  the next start reloads it from the store. It needs a persisted set.
- A forgotten conversation must come back when it matters. An incoming DM and a
  server-side force-join both have to clear the flag, the same way UI-039 handles
  `jabber_closed_dms` and `jabber_left_rooms`.
- Roster rows are not "known" in this sense. They come from the server and would be back
  on the next roster push, so the button belongs on remembered rows only.
- Forgetting a joined room implies leaving it, and must go through the UI-039 path so the
  leave sticks.

## How to verify

`cargo test --bin eve-spai jabber_forget` plus the two sidebar screenshots. The fix is
WRONG if forgetting deletes stored messages, if a forgotten conversation stays hidden
through a new incoming message, or if the button appears on a roster contact.
