# UI-051 &mdash; One dialog doing two jobs, offering the wrong conversations for both

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `app.rs` jabber sidebar, `uitest/scenes.rs` |
| **Reported by** | user |

## Symptoms

"The 'Join' button at the top of the sidebar should be obsolete now, due to the 'Start a DM' and
'Join a Room' buttons. The dialog should be split and not show both. Also the recent history includes
both DMs and rooms for either. Add some hover highlights to the recent ones as well and make it a
scrollable list, capping at a 100 entries."

## Cause

The sidebar button predates the two start rows added with the Convos rework and now opens the same
dialog they do, from a second place, with no way to say which half the user wanted.

The recent lists are the real defect. `f.convos` is built from every JID there is history for, and
`st.chats` holds rooms as well as people, so the DM list offered rooms. Nothing filtered it.

The recent entries were frameless buttons: the hit area and any hover were the width of the name, so
most of the row did nothing either way. Capped at six with no scroll, which hid everything older.

## How to verify

Open each half from its own row. The DM list must contain no rooms, the room list no people, and
every row must light up across its full width.
