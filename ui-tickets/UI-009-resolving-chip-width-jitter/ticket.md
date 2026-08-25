# UI-009 &mdash; Resolving-pilot chip shoves its row twice a second

| | |
|---|---|
| **Severity** | Low |
| **Status** | Fixed, see `review.md` |
| **Region** | `intel_row` |
| **Wave** | 4 |

## Symptom

The "Resolving pilot..." placeholder animates between one and three dots on a 450ms repaint. Its width changes with the dot count, so every chip to its right in the wrapped flow shifts horizontally, and can cross a line break and back twice a second.

## Cause

`app.rs:22183` builds the label as `format!("{} {dots}", icon::USER)` where `dots` is `".".repeat(phase)` with phase 1..3, then requests a repaint after 450ms.

## Notes

It only appears while pilots are unresolved, which is exactly when live intel is arriving and the feed is busiest.

## How to verify

The chip must hold a constant width across phases. Re-render `intel_row_torture.png` and confirm chip positions do not depend on the animation phase.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/intel_row_torture.png` (NOTE: does not actually show the chip, every torture pilot is resolved; see `review.md`)

After: `after/intel_row_resolving_phases.png`, `after/intel_row_resolving_phases.debug.png`
