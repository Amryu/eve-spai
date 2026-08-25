# UI-016 &mdash; Ping window has no chrome, unlike the alert window

| | |
|---|---|
| **Severity** | Low |
| **Status** | Closed, not a defect, see `review.md` |
| **Region** | `decision` |
| **Wave** | n/a |

## Symptom

The alert window has a title bar, a four-button cluster and a resize grip. The ping window has no header, no close or pin, and nothing in its bottom-right corner.

## Cause

Both scenes render the real viewport callback, so this is not a fixture crop. The two windows were built with different chrome.

## Notes

RESOLVED as a false positive: the ping window keeps the OS frame, the alert window sets with_decorations(false) and must draw its own. Original note follows.

Originally logged as: needs a product decision, not a fix. If the ping window is meant to be dismissed by the user or repositioned, it needs chrome. If it is meant to be transient, it does not.

## How to verify

n/a until the decision is made.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/ping_window_fleet.png`, `before/alert_window_typical.png`

After: recorded in `review.md` once fixed.
