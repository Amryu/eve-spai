# UI-014 &mdash; Copy button is smaller than every neighbouring control

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `render_ping` |
| **Wave** | 7 |

## Symptom

The Copy button is 69x18px against 115x28 for Join Mumble in the same card, and 28px for every intel chip and alert-window button.

## Cause

`app.rs:21326` and `app.rs:21393` use `small_button`.

## Notes

Check the census afterwards: the smallest hit target in the ping scenes is currently this button at 17px.

## How to verify

`uitest_census` must report a larger smallest-target for `ping_fleet`, and the card layout must not shift.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/ping_fleet.debug.png`, `before/ping_fleet.png`

After: recorded in `review.md` once fixed.
