# UI-007 &mdash; Alert window title bar cannot be grabbed where it looks grabbable

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | `alert_cb` |
| **Wave** | 4 |

## Symptom

The window drag rect spans x=7..390, y=7..35, but two click-and-drag label rects sit inside it at x=8..89 and x=96..109, covering the title text and the countdown. Grabbing the window by its title hits the labels, not the drag sense.

## Cause

"Intel alerts" and the seconds counter are rendered as selectable labels inside the title bar area, so they take the pointer before the drag rect does.

## Notes

`StartDrag` is issued from the drag rect. The labels do not need to be selectable.

## How to verify

`alert_window_typical.debug.png` must show no click-sensed widget inside the drag rect, and the layout must be unchanged.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/alert_window_typical.debug.png`, `before/alert_window_typical.png`

After: `after/alert_window_typical.png`, `after/alert_window_typical.debug.png`
