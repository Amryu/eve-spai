# UI-005 review cycle

**Status:** Fixed and verified
**Wave:** 3 (paired with UI-006 on `settings_view`, no region overlap)
**Branch:** `fix/ui-005-battles-wait-states`


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 7.9 min across 1 round, 37 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 63/5 lines (added/removed), excluding the harness |
| **Harness code changed** | 18/0 lines |
| **Suite** | 410 to 411 passing |
| **Follow-ups** | none |

## The change

New `battles_wait_note(&self, ui, waited)` replaces the unconditional spinner block, plus a
`battle_wait_since: Option<Instant>` field and a `BATTLE_STALL` const of 20s. Branch order, first
match wins:

| State | Shown |
|---|---|
| `!settings.battles_enabled` | "Battle reports are off. Tick Enabled to compute them." |
| worker never started, SDE downloading | spinner with the download text |
| worker never started, SDE failed | the failure reason, in `standing::WARNING` |
| worker never started, otherwise | "Battle reports have not started. They begin once the static data is downloaded." |
| started, nothing current for 20s | "No battle report after Ns. The background worker is not responding.", in warning colour |
| otherwise | the original spinner and "Loading battles…" |

## Review

The agent went past the ticket and found the more serious bug. I verified both claims in source
rather than taking them:

- **`out.ready` is not a completion signal.** `brview.rs:322` hardcodes `ready: true` in
  `compute()`, so the flag only ever means "the worker published at least once".
- **A disabled toggle is a genuine production hang.** `brview.rs:439` is
  `if !battles_enabled.load(..) { continue; }`, so while battles are off the worker skips the whole
  loop body and never publishes. Turn the toggle off with an empty card list and the old code spun
  forever, on a real machine, with no SDE problem in sight. The ticket only described the headless
  case.

The state model is right for the reason it gives: two of the three non-publishing cases are known
exactly from state the app already holds, so they answer immediately, and only a started-but-silent
worker needs a timer, because a wedged thread is indistinguishable from a slow one except by
elapsed time. A timeout as the sole mechanism would have made a disabled toggle take 20 seconds to
report something it knows instantly.

Settled branches also stop calling `request_repaint()`, so the app no longer burns frames forever
in a state that will never change.

## The genuine loading path still spins

This mattered more than the fix itself, since the easy way to kill a spinner is to kill it
everywhere. The agent verified by probe rather than by argument: it neutralised the
`!watcher_started` branch, re-rendered, and got a spinner plus "Loading battles…" pixel-identical to
the before shot. Then it set `BATTLE_STALL` to 0 and re-rendered to get the amber stall message.
Both probes were reverted and the final PNG re-rendered afterwards, which I confirmed in the diff.

## Coverage I added on review

The agent declined to add a test, reasoning that changing the test counts would break what I was
gating on. That was my prompt's fault, not its judgement, and it is the wrong trade: this ticket is
exactly the kind that regresses silently. Added
`uitest_battles_view_settles_without_a_worker`, which asserts the settled text is present and that
"Loading battles" is absent. Headless starts no worker, which is the same state a user hits with no
static data, so the scene reproduces it for free.

**Still uncovered:** the disabled-toggle branch and the 20s stall branch. Both need
`settings.battles_enabled` and a clock the harness can advance, which are GAP-002 and GAP-005.
Recorded rather than faked.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 410 passed, 2 ignored (+32) | **411 passed**, 2 ignored (+32) |
| Layout assertions | clean | clean |

`cargo check --workspace --all-targets --all-features`: only the pre-existing `unused_mut` at
`app/src/intel.rs:5605`.

## Screenshots

- `before/view_battles.png`: toolbars, then a spinner glyph beside "Loading battles…", over an
  empty panel that never changes.
- `after/view_battles.png`: identical chrome and toolbars, and in the same spot one weak line,
  "Battle reports have not started. They begin once the static data is downloaded." No spinner.

That message is the literally true answer for this state rather than a faked completion: the
headless profile has a scratch store with no SDE, so `sde_status` is `NotReady` and
`maybe_start_watcher` returns early.

## Rejected

- Seeding a fake "ready, zero battles" output in headless: makes the screenshot look settled while
  lying about a worker that does not exist.
- Putting the fix in `brview.rs`, for instance publishing an empty ready output when disabled: the
  worker is not running at all in the two most common cases, so no code there can speak for it.
