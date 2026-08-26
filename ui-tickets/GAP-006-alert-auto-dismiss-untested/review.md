# GAP-006 review cycle

**Status:** Closed
**Branch:** `harness/gap-006-alert-auto-dismiss`

## Resolution

| | |
|---|---|
| **Outcome** | Closed, test-only |
| **Agent time** | 10.2 min across 1 round, 51 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | **0 lines** |
| **Harness code changed** | 115/0 lines |
| **Suite** | 468 to 470 passing |
| **Follow-ups** | none |

## Why this gap mattered

Every alert scene set `pinned: true`, so `active` was permanently true and the auto-dismiss path had
never run. That path decides whether the overlay hands clicks back to the game. An overlay stuck
visible and click-absorbing over EVE is a serious failure, and it had zero coverage.

## No harness extension was needed

The agent checked rather than assuming: `Harness::run_steps` calls `step()` directly and never
consults `max_steps`, which only gates `_try_run` behind `run`/`run_ok`. So the 8-step cap in
`harness::build` does not limit stepping and `harness.rs` is untouched.

The new scene is not registered in `scenes::all()`, matching the existing probe scenes, so it adds
no PNG.

## My ticket was wrong about the commands

I wrote that expiry issues `Visible(false)` and `MousePassthrough(true)`. **Only the second is
unconditional.** I verified the code myself:

```rust
let want_visible = active || (st.enabled && !cfg!(target_os = "windows") && !st.dismissed);
let want_passthrough = !active;
```

Off Windows the overlay stays mapped when the countdown expires and merely becomes click-through.
`Visible(false)` is reserved for an explicit user dismiss and for Windows, where a
transparent-when-idle window renders as an opaque black square.

The agent asserted the platform-correct value through `cfg!` on both sides rather than asserting a
command the app deliberately does not send, and proved the Linux branch has teeth. Asserting my
version would have produced a test that passed only on Windows.

## What is asserted

While the countdown runs, the title and feed nodes are present. Then it steps one pass at a time,
recording the pass index of each command, until the title disappears:

- the feed node is gone
- `passthrough_at == expired_at`, so clicks are handed back on exactly the pass the window goes
  inactive
- `hidden_at == if cfg!(windows) { expired_at } else { None }`

Expiry took **16 passes** after `build` (build burns 4; 5.0s at 0.25s per step is 20 total).

## Teeth, four checks

| Break | Result |
|---|---|
| `active = st.enabled`, never expires | fails: "the countdown had not expired after 61 passes" |
| `active` pin condition **and** the `!pinned` decay guard both dropped | pin test fails: "the pinned alert window closed itself" |
| `want_passthrough = false` | fails: `left: None, right: Some(16)` |
| `want_visible = active`, unmapping on Linux too | fails: "hid on pass Some(16), wanted None" |

**I reproduced the third myself**, getting exactly `left: None, right: Some(16)`.

The second is the interesting one: dropping only `|| st.pinned` from `active` does **not** break the
pin, because the `!pinned` decay guard keeps `secs` frozen. The pin is held by two independent
conditions and both must break before the test fails. That is worth knowing before anyone
"simplifies" either one.

## Pinned-holds-open edge, also covered

`uitest_alert_window_pin_survives_the_countdown` runs 60 passes, three times the countdown, and
asserts the title still renders, the counter still reads `5s` (so the pin freezes the countdown
rather than merely overriding `active`), and `MousePassthrough(true)` was never issued. That
protects the existing scenes, which all depend on pinning.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 468 passed, 5 ignored (+32) | **470 passed**, 5 ignored (+32) |
| with `--features fc-rescue` | 495 passed | 497 passed |
| `cargo test --bin eve-spai uitest` | 55 passed | 57 passed |

**No rendering changed**, which is the bar for test-only work. The agent rendered all 100 PNGs at
`a0fd0c9` first, re-rendered after, and ran `cmp` on every file plus a directory diff: same 100
files, all byte-identical. Not even wall-clock digits moved, since the fixtures use a fixed
`fixtures::now()`.
