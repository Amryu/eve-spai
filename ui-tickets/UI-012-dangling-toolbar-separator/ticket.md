# UI-012 &mdash; Battles toolbar ends on a dangling separator

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `battles_view` |
| **Wave** | 5 |

## Symptom

A vertical divider is drawn as the last item of the first toolbar row with nothing after it, and the second row then starts with no leading divider.

## Cause

The toolbar is one `horizontal_wrapped` and the wrap point falls just past `toolbar_sep(ui)` at `app.rs:7903`.

## Notes

Width-dependent, so the artifact moves as the window resizes. A fix that only works at 1280px is not a fix.

## How to verify

`view_battles.png` must show no trailing divider. Check at more than one window width.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/view_battles.png`

After: recorded in `review.md` once fixed.
