# UI-029 review cycle

**Status:** Fixed and verified, with one residual gap recorded
**Branch:** `fix/ui-029-overlay-bridge-flag`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 10.2 min across 1 round, 46 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 121/19 lines (added/removed), excluding the harness |
| **Harness code changed** | 83/0 lines |
| **Suite** | 464 to 468 passing |
| **Follow-ups** | a writer-side test gap, recorded on GAP-007 |

## The change

One field carries the verdict, not the inputs, which was the right shape: the main process has
already done the graph work and the overlay has no graph to redo it with.

`AlertMsg` gains `#[serde(default)] pub via: Vec<JumpVia>`, index-aligned with the existing
`from_you`. `JumpVia` gains `Hash, Serialize, Deserialize`; no existing field changed type.
`push_overlay_update` computes `via` beside `from_you` and hashes it into the dedupe hash. The
shared render callback reads `via_pre.get(i).copied().unwrap_or_else(|| jump_via(..))`, so the
overlay uses the shipped verdict and the in-process viewport keeps computing locally.

Wire format gains `"via": ["Gates" | {"BridgeShorter": N} | "BridgeOnly", ...]` after `from_you`.

**One detail worth keeping:** `AlertPush` resizes `via` to the feed length after appending, because
a frame from an older main leaves it empty and a short vector would shift verdicts onto the wrong
cards. Getting that wrong would have been worse than the original bug.

## Compatibility, proved in both directions

- `alert_without_via_still_parses`: serializes an `AlertMsg`, strips the `via` key, asserts it
  deserializes with `via` empty and everything else intact. **New overlay, old main.**
- `alert_with_via_parses_where_the_field_is_unknown`: deserializes a `via`-carrying frame into a
  struct without the field, proving `AlertMsg` does not `deny_unknown_fields`. **Old overlay, new
  main.**
- `frame_roundtrip_alert_via_verdicts`: all three variants through the real `send`/`recv`.

The empty-`via` case degrades to exactly today's behaviour.

## What was tested, and what was not

**Tested:** the IPC round trip, both compatibility directions, and the alert render fed the way the
subprocess is fed. `alert_window_ipc_scene` builds an `AlertMsg`, pushes it through the real frame
codec, and applies it with `overlay::apply_alert` into a state with `player_sys: None` and
`count_bridges: false`, so **a mark on screen can only have come from the wire**. That is a good
construction. Its teeth are internal: the same test asserts no marks at all when `via` is empty,
which is the bug exactly as it shipped.

**Not tested:** the actual overlay subprocess. GAP-007 stands, kittest cannot spawn it. What is
covered is the same render callback and the same `apply_alert` the subprocess runs, reached
in-process. The agent stated this plainly rather than blurring it, which is what the brief asked for.

## Residual gap I found on review

**Nothing asserts that `push_overlay_update` populates `via`.** I checked by stopping it shipping
the field, and the overlay test still passed, because that test constructs an `AlertMsg` directly
and never runs the producer.

So the reader path (wire to render) is well covered including a negative case, but the writer path
is not. If someone later broke `push_overlay_update` to send empty verdicts, every test here would
still pass and the overlay would silently lose the mark again, which is precisely the bug this
ticket fixes. The code is correct today, verified by reading it. Recorded on GAP-007 rather than
left implicit.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 464 passed, 5 ignored (+32) | **468 passed**, 5 ignored (+32) |
| with `--features fc-rescue` | 490 passed | 494 passed |
| `ipc` round-trip tests | 6 | 9 |

## Screenshot

`after/alert_window_bridged.png`, three cards all reading `1j`: Jita `1j ⇆ bridge only`, 7-K5EL
`1j ⇆`, 319-3D plain `1j`. The agent sampled the glyph pixels rather than eyeballing the colour:
marked numbers are `#9B6FD8` (`standing::ALLIANCE`), unmarked `#4F9BD8` (`standing::CORP`). All
three numbers sit at one x.

`alert_window_typical.png` is unchanged and has no jump chip at all: that fixture seeds
`from_you = None` per card, which is why it could never have shown this bug and why a new fixture
was needed.

## Correction to my brief

I quoted the uitest floor as 51. It was 54. Second stale floor of the session, both off by 3. The
rule is now in CLAUDE.md: measure the count when writing the brief, never copy it forward.
