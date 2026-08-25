# UI-011 &mdash; Reporter footer flows inline with the badge chips

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `intel_row` |
| **Wave** | 5 |

## Symptom

The reporter and channel text starts on the same line as the last flag badge, then wraps to the card's left margin at a 15px line pitch against the badge rows' 34px. It reads as a cramped paragraph glued to the badge row rather than a footer. Three ragged lines at 320px.

## Cause

The footer is emitted into the same wrapped flow as the badge chips instead of starting its own row.

## Notes

Any long reporter and channel pair triggers this, not just the torture fixture.

## How to verify

`intel_row_torture.png` and `intel_row_torture_narrow.png` must show the footer starting on its own line, with spacing that reads as a footer.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/intel_row_torture.png`, `before/intel_row_torture_narrow.png`

After: recorded in `review.md` once fixed.
