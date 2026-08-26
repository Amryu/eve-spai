# UI-029 &mdash; The alert overlay cannot show the bridge flag

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | `ipc::AlertMsg`, `overlay.rs`, `AlertWindowState` |
| **Found by** | UI-026 |

## Symptom

UI-026 marks an intel card whose jump range depends on a jump bridge, so the reader knows the number
understates how far a hostile really is. **The alert overlay shows the number without the mark.**

That is the worst place to lose it. The overlay is what a user reads mid-fight, glancing away from
the game, and it is exactly where an understated distance does damage.

## Cause

`jump_via` needs the player's system and the bridge setting. In the overlay subprocess both are
absent:

- `AlertWindowState::player_sys` and `count_bridges` are only ever assigned in the main process
  (`app.rs:9098`, `app.rs:9109`).
- The overlay receives its numbers over IPC as `AlertMsg::from_you`, an already-computed jump count,
  and never learns either input.

So `jump_via` returns `Gates` in the overlay and no mark is drawn. The in-process alert viewport
path (`app.rs:9144`) flags correctly, so this only affects the shipping subprocess path.

## The fix

One field on `AlertMsg` carrying the `JumpVia` verdict (not the inputs; the main process has already
done the work and the overlay has no graph to redo it with), plus the matching assignment in
`overlay.rs`.

Note `ipc.rs` has round-trip tests for every message type, and per CLAUDE.md the wire format must
stay compatible: an old overlay and a new main process can coexist across an upgrade, so the field
needs `#[serde(default)]`.

## How to verify

The overlay subprocess cannot be rendered by the harness (GAP-007), so the in-process alert viewport
scene is the closest available, plus an `ipc.rs` round-trip test for the new field. That is a real
coverage limit, not an oversight: state it in the review rather than implying the overlay path was
tested.
