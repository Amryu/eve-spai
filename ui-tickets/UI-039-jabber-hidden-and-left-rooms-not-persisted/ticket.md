# UI-039 A hidden chat tab comes back, and leaving a room does not stick

- Severity: High
- Region: `jabber_reconcile`, `close_jabber_tab`, `jabber_frame` (app.rs); `handle_event` room arms and the `Cmd::JoinRoom`/`Cmd::LeaveRoom` arms (jabber.rs)
- Reported by: user
- Status: Open

## What happens

Closing a room tab asks "leave the room, or keep it joined and just hide the tab?".
Both answers leak.

**Hide.** The tab is back after a restart, on a room that is still joined. Reported as
"re-opens previously closed tabs after restarting"; the hide actually survives the
restart and is undone a few seconds later by the first message in the room.

**Leave.** There is no way to leave a channel for good. The leave holds for the session,
then the room is rejoined on the next start.

## Why

`jabber_reconcile` (app.rs) drops a conversation from `settings.jabber_closed_rooms` for
every key in `f.unread`:

```rust
// An incoming message (present in `unread`) reopens a conversation whose tab was closed.
for k in &f.unread {
    ...
    if let Some(p) = self.settings.jabber_closed_rooms.iter().position(|j| j == k) {
        self.settings.jabber_closed_rooms.remove(p);
```

That rule is right for a DM and wrong for a room. "Keep it joined and just hide the tab"
means the room keeps producing messages by design, so the first one after any reconnect
un-hides it. On a busy alliance channel the hide lasts seconds. It looks like a restart
bug because a restart is when it is most visible.

Leaving is a separate hole. `close_jabber_tab` sends `Cmd::LeaveRoom` and drops the room
from `settings.jabber_rooms`, but nothing records that the leave was deliberate, and
three paths put the room straight back:

1. `Event::RoomMessage` (jabber.rs) inserts any room it sees into `state.rooms`, to catch
   rooms the server force-joined us into without a `RoomJoined`. A message already in
   flight past our leave hits this.
2. `jabber_reconcile` then re-persists everything in `f.rooms` into
   `settings.jabber_rooms`, "so we rejoin it ourselves next time".
3. Next start, `maybe_start_jabber` joins every entry of `settings.jabber_rooms`.

So a left room is rejoined by us, not by the server, and the user has no way to say no.

Two smaller consequences of a leave that does stick, both of which have to be handled or
leaving looks broken in a different way:

- `dm_keys` in `jabber_frame` is "a chat key that is not a room", so a left room's stored
  history reappears as a DM tab.
- the Channels list is built from `settings.jabber_rooms` plus `state.rooms`, so a left
  room keeps a row that claims it is joined.

## Wanted

- A hidden room stays hidden across restarts and across ordinary room traffic. Being
  named in it is loud enough to bring the tab back; a plain message is not.
- Leaving a room is permanent: we never rejoin it ourselves, in this session or a later
  one, and it drops out of the Channels list and the DM list.
- The server can still put the user in a channel. A force-join, a bookmark join, or an
  invite overrides an earlier leave, which is the whole point for an alliance that
  mandates a channel.

## Before

The defect is state, not paint, so `before/` is the failing regression tests rather than
a screenshot: `hidden_room_survives_ordinary_traffic`, `leaving_a_room_is_permanent` and
`a_server_force_join_overrides_a_leave` in `mod jabber_room_tests`. See
`before/tests-fail-without-the-fix.txt`.
