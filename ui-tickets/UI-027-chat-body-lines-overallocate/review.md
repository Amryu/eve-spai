# UI-027 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-027-chat-body-rows`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 40.1 min across 1 round, 96 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 13/1 lines (added/removed), excluding the harness |
| **Harness code changed** | 109/0 lines |
| **Suite** | 455 to 458 passing |
| **Messages visible in a 520x480 popout** | **7 to 9** |
| **Follow-ups** | UI-028, a fourth instance in the rescue chat line |

## The change

Two edits, 14 lines added, 1 removed.

1. The `horizontal_wrapped` in `jabber_conversation_ui` holding nick and body becomes the
   zero-floored wrapping layout. **That container is what floors the row**, which is why
   `render_message_body` could not fix it from inside: egui reads `interact_size.y` off the parent
   as the child's `desired_size.y`, which becomes `first_row_min_height` for the galley.
2. An empty body emits nothing, so a tight row is literally 0.0px and the message folds into the one
   above. Guarded with `allocate_space`.

The second point is UI-018's blank-line lesson **in the shape chat needs**, not a copy of it.
UI-018's `add_space` works because ping lines sit in a vertical parent; in chat the body sits inside
the horizontal row, where `add_space` advances x rather than y. Recognising that the same lesson
needed a different mechanism is the part that mattered.

## Measurements

| | before | after |
|---|---|---|
| one-line body row | 26.0 | **15.0** |
| nick label on the same row | 26.0 | **15.0** |
| wrapped 2-row body, 520px | 41.0 | **30.0** |
| wrapped 3-row body, 360px | 56.0 | **45.0** |
| grouped pitch, body top to body top | 29.0 | **18.0** |
| ungrouped pitch, timestamp to timestamp | 46.0 | **35.0** |
| 16-message history content height | 739 | **563** |

UI-018 read it correctly: only the first row was floored, so a wrapped body is now an exact multiple
of 15.0.

## Performance, checked because UI-022 just optimised this surface

| | ms/frame, release |
|---|---|
| before | 0.22 |
| after | **0.25** |

**I reproduced 0.25 myself.** That is a real ~10% increase, and the agent diagnosed it rather than
waving it off: rows built per pass go 18 to 25 of 1000, because shorter rows fit more messages inside
viewport plus `MSG_OVERDRAW`. That is virtualization behaving correctly on denser content, not
degrading. Still 17x under the 4.31 ms pre-UI-022 baseline, and the bounded-render assertion passes
with an order of magnitude of headroom.

All three UI-022 long-history tests stay green, which is what I most wanted to see given the height
cache.

## Every `render_message_body` caller

| line | caller | outcome |
|---|---|---|
| 4027 | `jabber_conversation_ui` | fixed, 26.0 to 15.0 |
| 21425 | `rescue_chat_line` (`fc-rescue`) | **untouched, still floored**. Fourth instance of the same root cause, out of scope, filed as UI-028 |
| 24181 | `ping_bodies_render_without_panicking` | unchanged. Its body list contains `""`, so it now also covers the empty-body branch |
| 24288 | `hovering_a_row_does_not_reflow_it` | unchanged |
| 24336 | `message_rows_render_in_every_state` | unchanged |

## Teeth

Three new tests, each verified to fail on the specific behaviour reverted. **I re-ran the check
myself** after botching my first attempt by passing two filters to `cargo test`, which takes one:

- `a one-line body is 26.0px tall, still floored at interact_size 26.0`
- `a wrapped body is 41.0px tall, not a whole number of 15.0px rows, so its first row is still floored`
- `the empty message opened 6.0px between its neighbours, less than the 15.0px of the line it stands for`

## The scroll round-trip blind spot

No more relevant than before, and the agent reasoned it through rather than asserting it. The spacer
is still measured off the live layout and the cache key still hashes width and content, so nothing
changed about how heights are produced, only their values. The known blind spot is a spacer wrong by
a constant factor relative to its row, and this change scales row and recorded height together by
construction. If anything the test is slightly stronger, since 25 rows per pass are built rather
than 18.

## Screenshots

- `after/jabber_popout.png`: 9 whole messages against 7 before, timestamps sitting tight on the
  message they label rather than adrift above it. It reads as a chat log instead of a list. The
  `— new —` divider is still between the right two messages and grouping still suppresses repeated
  nicks.
- `after/jabber_popout_long.png`: tail still on #999, divider present, grouped runs of three under
  one nick, even leading throughout.

## Incident during this ticket

This agent's `git stash` clobbered the UI-026 agent's worktree, because `refs/stash` is shared
across all worktrees of a repository. It noticed, rescued the other agent's work to a patch, verified
that patch applied cleanly, and **led its report with the incident** rather than burying it. Both
rules are now in CLAUDE.md. The disclosure is why nothing was lost.
