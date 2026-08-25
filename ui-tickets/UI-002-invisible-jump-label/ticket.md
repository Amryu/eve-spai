# UI-002 &mdash; Every intel card carries an invisible 34x28 click target

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | `intel_row` |
| **Wave** | 2 |

## Symptom

A 34x28 rect at x=99..132, y=12..39 on every intel card contains zero painted pixels, yet it is click-and-drag sensed and reserves horizontal space. On the 320px card it eats ~33 of 288 usable pixels and is part of why the first row fits only one system chip.

## Cause

`app.rs:21826` runs `ui.label(RichText::new(format!("{jtxt:>4}")).monospace())` unconditionally. When `from_you` is `None`, `jtxt` is empty, so this renders four monospace spaces.

## Notes

The `show_raw` branch immediately above handles its `None` case correctly. Match that shape rather than inventing a new one.

## How to verify

The empty rect must be gone from `intel_row_typical.debug.png`, and the 320px card must gain the reclaimed width. Cards where `from_you` is `Some` must keep their existing alignment.

Re-render with:

```bash
cargo test --bin eve-spai uitest                              # assertions must stay green
cargo test --bin eve-spai uitest_screenshots -- --ignored     # writes target/uishots/
```

## Screenshots

Before: `before/intel_row_typical.debug.png`, `before/intel_row_torture_narrow.png`

After: `after/intel_row_typical.debug.png`, `after/intel_row_torture_narrow.png`, `after/alert_window_torture.png`
