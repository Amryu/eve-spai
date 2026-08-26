# UI-010 &mdash; `uncertain` is silently keyed by lowercase

| | |
|---|---|
| **Severity** | Low |
| **Status** | Fixed, see `review.md` |
| **Region** | `intel_row` |
| **Wave** | 7 |

## Symptom

Passing correctly-cased pilot names in the `uncertain` set makes the uncertain-pilot marker silently never appear. The harness's own fixture hit this and the feature went unrendered without any error.

## Cause

`app.rs:22108` looks the set up with `name.to_lowercase()`, but the parameter is a plain `&HashSet<String>` with nothing in the type or the name to signal the contract.

## Notes

Make the contract impossible to get wrong, or at minimum impossible to miss. A newtype, a rename, or a doc comment on the parameter are all acceptable; pick one and say why in the review.

## How to verify

`intel_row_typical.png` must still show the amber "?" chip on Second Target. A caller passing mixed-case names must either work or fail loudly.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/intel_row_typical.png`

After: `after/intel_row_typical.png`
