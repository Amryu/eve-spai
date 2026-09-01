# UI-039 review cycle

**Status:** Fixed and verified
**Branch:** `ui-039-jabber-leave-and-hide-not-persisted`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | one session, no fix agent dispatched (single region, cross-cutting across app.rs and jabber.rs) |
| **Patches rejected on review** | 0 |
| **App code changed** | +215 / -15, of which 128 lines are the new test module |
| **Harness code changed** | 0 |
| **Suite** | 555 to 562 (`cargo test --features fc-rescue`), uitest 76 passed |
| **Follow-ups** | none |

## What changed

One new setting, `jabber_left_rooms`, and one new field on `JabberState`, `rooms_left`. The
setting is the durable record, the field is the live one, and `jabber_reconcile` keeps them
in step the way it already does for `jabber_inaccessible_rooms`.

The two reported symptoms had separate causes.

**Hidden tab comes back.** `jabber_reconcile` un-hid a room for every key in `f.unread`. Split
that loop: `f.unread` still un-hides a DM, `f.mentions` un-hides a room. "Keep it joined and
just hide the tab" now means what it says, and being named in the room is still loud enough to
surface it.

**Leaving does not stick.** `close_jabber_tab` now records the leave in `jabber_left_rooms` and
calls `jabber::note_room_left`, which drops the room from `rooms` and adds it to `rooms_left`.
`jabber_rooms_to_join` filters the left set out of the connect-time join list, so we never
rejoin it ourselves. Rejoining by hand goes through `jabber_unleave`, from both the join dialog
and the Channels list Join button.

The three room bookkeeping sites in `jabber.rs` became named functions, `note_room_joined`,
`note_room_seen` and `note_room_left`, because the tests need to drive exactly those state
transitions and a `#[cfg(test)]` shim would have been untestable by the revert check. That is
also where the force-join rule lives: `note_room_joined` clears `rooms_left`, `note_room_seen`
refuses to. Self-presence from the MUC is the signal that the server actually put us back in a
room, and a straggling `RoomMessage` is not, so a message in flight past our leave no longer
resurrects the room while an alliance-mandated force-join still does.

Two consequences of a leave that sticks, both fixed here because leaving looks broken without
them: `dm_keys` filters `rooms_left` (a left room's stored history was surfacing as a DM tab,
since a DM key is defined as "a chat key that is not a room"), and the Channels list drops left
rooms (they were still listed as joined, from `settings.jabber_rooms`).

## Rejected

Recording the leave only in the worker's `joined` set. It survives a reconnect but not a
restart, which is half the bug, and `close_jabber_tab` has to work with `jabber_tx` unset
(user leaves a room while disconnected).

Dropping the reopen-on-message rule for DMs too. Nobody asked for that, and a DM tab is closed
because the conversation is over, not to mute a channel that keeps talking.

Suppressing left-room messages before `push_msg` rather than at `note_room_seen`: the early
return in the `RoomMessage` arm covers both, and putting the guard in the shared helper keeps
one definition of "we are not in this room".

## Teeth

`before/tests-fail-without-the-fix.txt` is the suite with the behaviour reverted and the tests
left in place. Four of seven fail:

- `hidden_room_survives_ordinary_traffic`, after restoring the `f.unread` loop over
  `jabber_closed_rooms`
- `leaving_a_room_is_permanent`, after removing the `jabber_left_rooms` push and the
  `note_room_left` call, and the `rooms_left` guard in `note_room_seen`
- `a_left_room_is_not_joined_on_the_next_start`, after making `jabber_rooms_to_join` return
  `jabber_rooms` verbatim
- `a_left_room_is_neither_a_dm_nor_a_channel_row`, after removing the `rooms_left` filters in
  `jabber_frame`

The other three (`being_named_in_a_hidden_room_brings_the_tab_back`,
`a_closed_dm_still_reopens_on_a_plain_message`, `a_server_force_join_overrides_a_leave`) pass
in both states by design. They pin behaviour the fix had to preserve, not behaviour it added.

## Screenshots

`after/jabber_popout.png` and `after/jabber_popout_dm.png` show the tab bar and history
unchanged, which is the point: nothing about this fix should move a pixel in a window whose
rooms are all still joined. The defect itself is state that only shows across a restart, so the
tests are the signal here, not the PNGs. The close-room dialog wording did change ("leave the
room for good", plus a sentence saying what a leave now means), and that dialog is unreachable
from the harness under GAP-001.

## Residual risk

A room the server keeps in the user's bookmarks is rejoined by the server on every connect, so
leaving it locally holds only until the next `RoomJoined`. That is the requested behaviour, the
server wins, but a user who leaves such a room will see it come straight back and may read that
as this bug. Worth a Channels-list affordance if it comes up; not filed, since no one has hit it.

`jabber_left_rooms` grows without bound. So do `jabber_closed_rooms` and `jabber_closed_dms`,
which have the same shape and have not been a problem.
