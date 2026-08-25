# UI-003 &mdash; Fleet ping body renders dimmer than a routine reminder

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `render_ping` |
| **Wave** | 1 |

## Symptom

The fleet call body ("Strat op, capitals staged...") renders at ~4.9:1 contrast while its own metadata labels render at ~11:1, and a routine plain-ping reminder below it also renders at ~11:1. In `ping_window_mixed.png` the two are stacked and the operationally urgent text is visibly the weaker of the two.

## Cause

`app.rs:21380` calls `render_ping_body(ui, description, true)` where the third parameter is `weak`. `app.rs:21402` calls it with `false` for the plain ping.

## Notes

This is an urgency hierarchy question, not a styling preference. The fleet call is the more important message.

## How to verify

In `ping_window_mixed.png` the fleet body must read at least as strong as the plain body below it.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/ping_window_mixed.png`, `before/ping_fleet.png`, `before/ping_plain.png`

After: recorded in `review.md` once fixed.
