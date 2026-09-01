# UI-041 review cycle

**Status:** Fixed and verified
**Branch:** `ui-041-forget-known-conversation`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | one session, no fix agent dispatched (one behaviour spanning both sidebar panes) |
| **Patches rejected on review** | 0 |
| **App code changed** | +105 / -4, plus 181 lines of test module |
| **Harness code changed** | +50 (a scene builder, two scenes, one fixture) |
| **Suite** | 564 to 571 |
| **Smallest hit target, Channels pane** | 13px to 19px (the button is no longer the smallest thing in it) |
| **Follow-ups** | UI-042 |

## What changed

One setting, `jabber_forgotten`, holding room and DM JIDs, and one action, `jabber_forget`,
reachable from a button on every removable row in both sidebar panes.

`jabber_forget` drops every persisted trace of the conversation: `jabber_rooms`,
`jabber_closed_rooms`, `jabber_closed_dms`, `jabber_inaccessible_rooms`, the saved MOTD in
`jabber_room_subjects`, and the contacts entry. It clears the live MOTD and the unread and
mention marks, closes the tab through `remove_jabber_tab` (which covers pop-outs, and leaves
an emptied pop-out for `jabber_reconcile` to prune), and drops the selection if the removed
conversation was the one on screen. A room that is currently joined is left first, through
the same `note_room_left` path UI-039 built, so the leave sticks across a restart.

Stored messages are untouched, per the user's choice. That is what forced the persisted set:
hiding by deleting from `st.chats` would last until the next start, when the store reloads it.

Three lists filter on it, which is what makes a removal look complete: `convos` (the Directory
pane, where remembered conversations land in the catch-all "Other" group), `dm_keys`, and the
`known` set behind the Channels pane.

`Convo` gained `in_roster`. The Directory pane offers the button only on rows that came from
chat history, never on a roster contact, because the roster is the server's and the contact
would be back on its next push. The Channels pane offers it on every row, including the
struck-through history-only ones, which had no per-row control at all before.

Removing is undone by anything that puts the conversation back in front of the user: an
incoming message (`f.unread` in `jabber_reconcile`), a server-side force-join (`f.rooms`),
joining the room by name, or opening the DM by name. Removal is curation, not a mute, and a
row that silently swallowed new mail would be a worse bug than the one being fixed.

## Rejected

Reusing `jabber_left_rooms`. It means "do not rejoin", it is rooms-only, and a left room is
deliberately still offered in the browse results so the user can rejoin it. Removal has to
cover DMs and has to hide the row everywhere.

Deleting the stored messages. Offered as an option, declined by the user in favour of keeping
them. Rejoining a removed room now restores the full backlog.

Matching the contacts star's `.small()` sizing for visual consistency. Measured instead: the
census put the star at 9px wide and the small forget button at 13px, both under the app's
~27px norm. The button now uses default glyph size and `min_size(24, 24)`; the star is UI-042.

## Teeth

`before/tests-fail-without-the-fix.txt` is the suite with the persisted set and its three
filters removed, tests left in place. Two of six fail:

- `forgetting_a_room_acts_as_if_we_were_never_in_it`, on `jabber_forgotten` staying empty
- `a_forgotten_dm_comes_back_on_a_new_message`, on the same

`forgetting_a_joined_room_leaves_it`, `forgetting_keeps_the_chat_history`,
`a_forgotten_room_comes_back_on_a_force_join` and
`roster_rows_are_not_forgettable_but_remembered_ones_are` pass in both states, because they
pin the UI-039 machinery this fix reuses rather than the new set.
`forgetting_closes_the_tab_in_a_popout_too` was added after the teeth run, on the user's
request that removal close the tab, and covers the pop-out case plus the empty-window prune.

## Screenshots

`after/jabber_sidebar_channels.png`: three rooms, each with the circled X at the right of its
name, including `ancient.op` struck through with its stale MOTD underneath. That row was
previously unremovable by any means.

`after/jabber_sidebar_directory.png`: the "Fleet" group's roster rows carry only the star,
while "Other" (corp.chat, Random Guy) carries star and X. That contrast is the whole point of
`in_roster` and is why both panes got a scene rather than one.

Both scenes are new. The chat sidebar had no harness coverage at all before this ticket, which
is also how UI-042 got measured.

## Residual risk

`jabber_forgotten` grows without bound, like `jabber_closed_rooms` and `jabber_closed_dms`
before it.

There is no undo and no confirmation. Removal is cheap to reverse by joining the room or
messaging the person again, and the history survives, so a confirm dialog seemed like the
wrong trade. If someone misclicks on a room whose JID they cannot remember, they have to find
it again through Browse server rooms.

A removed room that the server keeps in the user's bookmarks comes back on the next connect,
by design (UI-039), and the user sees no explanation for why.
