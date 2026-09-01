# UI-047 review cycle

**Status:** Fixed and verified
**Branch:** `ui-047-force-join-opens-once`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed, and the Rescue Mode guarantee was strengthened at the user's request |
| **Agent time** | one session, no fix agent dispatched |
| **Patches rejected on review** | 0 |
| **App code changed** | +50 / -30, plus 190 lines of test module |
| **Harness code changed** | 0 |
| **Suite** | 595 to 601 with `fc-rescue`, 562 without |
| **Follow-ups** | none |

## What changed

**A force-join opens the tab once.** `jabber_reconcile` already had the marker: it copies any
room in `f.rooms` that is not yet in `settings.jabber_rooms` into it, and that branch cannot
fire twice, because the next frame finds the room recorded. Rooms the user joins by hand never
reach it, since the join dialog and the Channels-list Join button write `jabber_rooms` before
the join lands. Those rooms are now collected and added to the wanted tab set, and their
`jabber_closed_rooms` entry is cleared on the way, or the tab would be opened and then pruned by
the `want` filter on the very next frame.

**Rescue Mode's rooms are held open, not merely joined.** The user escalated the requirement
mid-ticket, from "forced to be active" to "always ensured to be open and working". delve911 and
skirmish_commanders are now pushed into the wanted set on every reconcile, `close_jabber_tab`
refuses them, and the healing step also clears any stale `jabber_closed_rooms` entry, which is
state nothing can act on once a room cannot be hidden.

That reverses UI-045's "hiding them is still allowed", which was the right call when pinning
only meant joined. It is recorded here rather than edited into UI-045.

## Rejected

Tracking force-joins in their own persisted set. `jabber_rooms` already answers "have we seen
this room before" exactly once per room, and a second list would be a second thing to keep in
step with the first.

Opening a tab for every room in `f.rooms` missing from the tab bar. That is the UI-046 bug
again: it fires every frame, not once, and resurrects anything the user closed.

Letting the pinned rooms keep a closable tab that silently reopens next frame. A close button
that undoes itself reads as a bug. `close_jabber_tab` returns early instead, and the sidebar
remove button already explains itself with a disabled tooltip.

## Teeth

`before/tests-fail-without-the-fix.txt` is the suite with the force-join surfacing removed and
the pinned rooms back to merely joined. Eight fail, including `a_force_join_opens_the_tab`,
`it_opens_once_and_the_close_sticks`, `a_force_join_survives_a_stale_hidden_flag`,
`the_pinned_rooms_cannot_be_closed` and `the_rescue_rooms_are_open_and_working_end_to_end`.

`an_already_known_room_is_not_a_force_join` and `a_hand_joined_room_does_not_double_open` pass
in both states by design: they assert the absence of an opening, which is what UI-046 established.

`it_opens_once_and_the_close_sticks` is the one that matters. It opens by force-join, closes,
runs five more reconciles, then rebuilds the app from the saved settings with the room still
joined, and asserts the tab is still shut. "Once" is the requirement, and a test that only
checked the first open would have passed against the UI-046 bug.

## The re-verification the user asked for

`the_rescue_rooms_are_open_and_working_end_to_end` walks the whole chain in one test, starting
from the worst profile: both rooms left, forgotten, and hidden, no tabs, nothing in
`jabber_rooms`.

| Link | Asserted |
|---|---|
| Repaired before connecting | `jabber_rooms_to_join` returns both after an offline reconcile |
| Joined | both in `JabberState::rooms` after `note_room_joined` |
| Receiving | `note_room_seen` returns true for both, which is the gate that feeds `push_msg` and therefore the rescue parser |
| Open | both hold a tab after reconcile |
| Un-leavable | `jabber_forget` on each is a no-op |
| Un-closable | `close_jabber_tab` on each is a no-op |
| Still open after all that | both tabs survive a further reconcile |
| Across a restart | both are in `jabber_main_tabs` after the mirror |

## Residual risk

Everything above assumes the server lets us in. A pinned room we are banned or kicked from lands
in `rooms_inaccessible` and Rescue Mode degrades quietly, with a struck-through channel row as
the only hint. UI-045 flagged this and it is still true; a rescue-side health check that says
out loud "delve911 is not joined" is the missing piece, and it belongs with the rescue window
rather than here.

A force-join surfaces a tab but no sound or badge beyond the room's own unread marker. If the
alliance force-joins a channel during a fight, a new tab is easy to miss.
