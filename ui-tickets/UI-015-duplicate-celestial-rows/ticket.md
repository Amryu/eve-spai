# UI-015 &mdash; Near-duplicate celestial printed on two rows

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `intel_row` |
| **Wave** | 6 |

## Symptom

A report carrying both `near_celestial` and `celestials` renders "Moon 6-3 - Chemical Laboratory  0 km" and "Planet VI - Moon 3 - Blood Raider Chemical Laboratory" on adjacent rows, wasting a whole row on near-identical content.

## Cause

The two fields are rendered independently with no check for overlap between them.

## Notes

Real reports can carry both fields, so this is not only a fixture artifact. Matching them needs care: the strings are similar, not equal.

## How to verify

`intel_row_torture.png` must show one celestial row, without losing the distance readout.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/intel_row_torture.png`, `before/alert_window_torture.png`

After: recorded in `review.md` once fixed.
