# UI-036 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-036-chat-scroll-shared-across-tabs`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none, done inline |
| **Patches rejected on review** | 0 |
| **App code changed** | 4/1 lines, the `ScrollArea` salt in `jabber_conversation_ui` |
| **Harness code changed** | 158/0 lines: two assertions and one screenshot scene |
| **Tests** | 510 to 512 passing, 4 to 6 ignored |
| **Top message on arrival** | `#0` of 1000 to `#990` |
| **Follow-ups** | none filed, one latent defect recorded below |

## What changed

`.id_salt("msgs")` became `.id_salt(("msgs", jid.as_str()))`. The `ScrollArea` now keys its state by
conversation instead of only by window, so each tab owns its offset and its `scroll_stuck_to_end`
flag.

The ticket's cause holds up in `egui-0.34.3/src/containers/scroll_area.rs`. Line 1510 only re-enters
sticky mode from a body that fits while `stick_to_end` is being asked for, and
`jabber_conversation_ui` withholds `stick_to_bottom` on any frame the pointer is down. A tab press
is such a frame, and a two-message DM is such a body, so the DM handed the shared state
`stuck = false, offset = 0`. Line 1245 then declines to pull the room to its end because the flag is
false, and the recompute keeps it false, which is why the room stayed pinned at message #0 rather
than snapping back on the next message.

## What was rejected

Forcing the history to the bottom on every tab switch. It fixes the screenshot and breaks the
conversation you deliberately scrolled back in, so
`uitest_jabber_tab_switch_keeps_a_scrolled_back_conversation` exists to fail that fix.

Widening the `selecting` guard was also left alone. It was added so an incoming message does not
wipe a drag-selection, the report here is about tab switching, and the two interact only through the
shared state that the salt removes.

## Teeth

Both tests were run against `.id_salt("msgs")` restored, nothing else changed:

- `uitest_jabber_tab_switch_opens_at_the_newest_message` failed with
  `drew: ["gate is clear #0", ... "#5"]` against the expected `#999`.
- `uitest_jabber_tab_switch_keeps_a_scrolled_back_conversation` failed
  `left: Some("gate is clear #0"), right: Some("gate is clear #892")`.

The second test needed `run_steps(16)` after the wheel events. egui animates a wheel scroll, so
reading the landing position four frames in recorded `#894` while the offset was still moving, and
the comparison drifted by two messages against a fix that was working.

## Screenshots

`before/jabber_tab_switch_to_room.png`: the room opens on `me: gate is clear #0` at 13:39, the
oldest of 1000 messages, with the twelfth message clipped by the composer. Nothing on screen says
989 messages sit below.

`after/jabber_tab_switch_to_room.png`: the same click lands on `#990` through `Logi Lead: warp to me
#999` at 15:20, with the "new" divider in view where the session boundary falls.

`*_from_dm.png` in both folders is the two-message DM the switch starts from, identical either way,
which is the point: the short side of the swap never showed the fault.

## Residual risk

Each open conversation now keeps a `scroll_area::State` entry. egui garbage-collects ids it has not
seen for a while and a window holds a handful of tabs, so this is not a leak worth guarding.

One latent defect stays open, recorded in the ticket's Notes. Any pointer press while a conversation
shorter than the window is showing clears that conversation's sticky flag, through the same
`selecting` gate and the same egui line 1510. Once that conversation grows past a screenful it stops
following new messages until the user scrolls to the bottom by hand. Per-conversation state makes it
persist rather than being reset by the next tab switch, so this fix makes it slightly easier to hit.
Fixing it means narrowing `selecting` to a drag that started inside the history, which trades against
the selection behaviour that put the guard there, so it is left for its own ticket.
