# UI-004 &mdash; Alerts toolbar reads "zKill intel within feed"

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | `alerts_view` |
| **Wave** | 2 |

## Symptom

The toolbar row reads `[x] zKill intel  within  [feed]`, which is broken English at the control's default value of 0. The DragValue also shows no arrows or drag affordance, so "feed" reads as an inert button.

## Cause

`app.rs:1432` gives the DragValue a `custom_formatter` that renders 0 as the word "feed". At any non-zero value it reads "within 5j" and is fine. The same formatter is used in the intel toolbar at `app.rs:5661`.

## Notes

Fix both sites. The zero case needs different sentence construction, not a different word.

## How to verify

`view_alerts.png` must read as grammatical English at the default value, and the control must look adjustable.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/view_alerts.png`

After: `after/view_alerts.png`, `after/view_alerts.debug.png`
