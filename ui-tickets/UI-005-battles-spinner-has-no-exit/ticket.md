# UI-005 &mdash; Battles spinner has no exit path

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `battles_view` |
| **Wave** | 3 |

## Symptom

The battles view shows "Loading battles..." indefinitely. There is no error state and no empty state, so a load that never completes is indistinguishable from one still in progress.

## Cause

`app.rs:8052` shows the spinner whenever `!ready || !fresh`, and `app.rs:8044` is the only place `battle_cards_ready` is ever assigned, from `out.ready`. If the worker never produces output, the flag never flips.

## Notes

Guaranteed in headless mode, where the worker is disabled by design. That makes it easy to reproduce, but it is a real production failure mode too.

## How to verify

`view_battles.png` must show a settled state rather than a spinner when no worker output arrives. A distinct empty state and error state are both acceptable; a permanent spinner is not.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/view_battles.png`

After: recorded in `review.md` once fixed.
