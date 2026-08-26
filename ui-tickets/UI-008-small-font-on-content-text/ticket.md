# UI-008 &mdash; Sixteen .small() sites on content text

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | `cross-cutting` |
| **Wave** | 8 |

## Symptom

Content text is rendered at roughly half body size in 16 places, including the wormholes empty-state subtitle, the wormhole table column headers, the drifter tag, the ping card footer, and (INCORRECTLY) the alert window countdown, which uses .weak() only. See `review.md`. Contrast is fine at about 4.8:1; size is the problem.

## Cause

16 `.small()` call sites in `app/src/app.rs`.

## Notes

The project convention is explicit: never use small font sizes for content text. Cross-cutting, so this ticket runs alone with no other agent in flight. Chrome that is genuinely not content may stay small if justified in the review.

## How to verify

Every affected screenshot must show the text at default body size. `cargo test --workspace` stays green.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/view_wormholes.png`, `before/ping_fleet.png`, `before/alert_window_typical.png`

After: `after/view_wormholes_rows.png`, `after/view_wormholes_rows_narrow.png`, `after/view_wormholes.png`, `after/ping_fleet.png`
