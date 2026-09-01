# UI-045 review cycle

**Status:** Fixed and verified
**Branch:** `ui-045-close-hides-and-rescue-room-pinned`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed, and the ticket's diagnosis of the user's report was corrected on the way in |
| **Agent time** | one session, no fix agent dispatched |
| **Patches rejected on review** | 0 |
| **App code changed** | +82 / -100, a net deletion; 150 lines of test module |
| **Harness code changed** | +24 (one scene, two fixture rows) |
| **Suite** | 577 to 585 with `fc-rescue`, 548 without |
| **Ways a tab close can leave a room** | 1 to 0 |
| **Follow-ups** | none |

## The report was not what it looked like

Reported as "chats are still reopening after restarts", which reads as a UI-039 regression. It
was not. Measured against the reporting user's live profile, read-only: zero rooms rejoined by
the server, zero rooms joined that settings did not ask for, and 14 of 15 stored DM
conversations correctly marked closed. UI-039 was doing its job.

The real state was `jabber_closed_rooms: 0` alongside `jabber_close_room_leaves: true`. Their
answer to a one-time prompt, given long before that answer became permanent, had been turning
every tab close into a channel leave ever since. `close_jabber_tab` only raised the prompt when
the value was `None`, so nothing ever reached `jabber_closed_rooms` and `jabber_reconcile`
reopened all five joined rooms on every start. The user was not failing to keep tabs closed,
they had no way to close one.

Worth recording as a diagnostic pattern: the fix shipped, the symptom persisted, and the cause
was a stale setting the fix never touched. Reading the reporter's actual settings took a minute
and pointed somewhere the code alone did not.

## What changed

**The X always hides.** `close_jabber_tab` no longer branches. `jabber_close_room_leaves`, its
three-way selector in the Jabber alerts window, `jabber_close_room_prompt` and the whole
`jabber_close_room_dialog` are gone. Leaving is the sidebar remove button from UI-041 and
nothing else, which is a single place instead of two paths with different destructiveness
behind the same glyph. `Settings` has no `deny_unknown_fields`, so an existing config carrying
the dropped key deserializes fine and simply loses it on the next save.

**Both rescue rooms are pinned.** `jabber_rescue_rooms` returns the configured delve911 and
skirmish_commanders JIDs, but only in an `fc-rescue` build with `fc_rescue_enabled` set. They
are force-joined by `jabber_rooms_to_join` even if they are in `jabber_left_rooms`,
`jabber_forget` refuses them, and the sidebar renders their remove button disabled with a
tooltip saying why rather than dead.

`jabber_reconcile` heals a profile that already left or forgot one, before its
configured/ever_online early return, so the join list is right on the next connect rather than
one round trip later.

## Why pinning was needed at all

`ingest_delve911_jabber` reads `st.chats[delve911_jid]`, and the rescue window reads the
skirmish room's tail and posts `!bping` requests into it. Since UI-039, a left room's messages
are dropped in `note_room_seen` before `push_msg` stores them. So leaving either room left
Rescue Mode running, connected, and permanently silent: no parsed events, no ping feed, no
ship-horn. A capital-rescue tool failing closed from one click on an X, with nothing logged,
because leaving is what the user appeared to ask for.

Hiding is deliberately still allowed on both. It keeps the room joined and the messages
flowing, so it costs nothing, and taking it away would have punished the safe action to prevent
the dangerous one.

## Rejected

Keeping "leave" on the X behind a confirmation. The prompt is what caused this: it was answered
once and then never seen again. Two actions with different consequences should be two controls,
not one control with a remembered mode.

Blocking the pinned rooms by making the button vanish. A missing control reads as a bug; a
disabled one with "Rescue Mode needs this channel" explains itself and points at the off switch.

Keying the pin on rescue mode being *active* rather than *enabled*. The backlog has to already
be there when an emergency starts, so the pin follows the setting.

## Teeth

`before/tests-fail-without-the-fix.txt` is the suite with the leave branch restored in
`close_jabber_tab` and `jabber_rescue_rooms` stubbed to empty. Seven fail, including
`closing_a_room_tab_only_hides_it` and every pinning test.
`nothing_is_pinned_with_rescue_mode_off` passes in both states, as it must: it asserts the
absence of the new behaviour.

## Screenshots

`after/jabber_sidebar_rescue_pinned.png`: five channel rows, `delve911` and
`skirmish_commanders` with visibly dimmed remove buttons, `delve.imperium`, `corp.chat` and the
struck-through `ancient.op` with live ones. `after/jabber_sidebar_channels.png` is the same
pane with Rescue Mode off, where all five are live. The scene is `fc-rescue`-gated, since
without the feature there is nothing to pin.

## Residual risk

An existing profile that already left one of the two rooms heals on the first reconcile, but a
user who deliberately left skirmish_commanders and does not want it back has no way to say so
except turning Rescue Mode off. That is the requested precedence.

Pinning force-joins on connect. If the server refuses the join (kicked, banned, room gone),
the room lands in `rooms_inaccessible` and Rescue Mode is quietly degraded again, with only the
struck-through channel row as a hint. A rescue-side health check belongs somewhere, but not in
this ticket.
