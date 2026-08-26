# UI-017 &mdash; Work-throttle ComboBox overflows the window at wide widths

| | |
|---|---|
| **Severity** | Low |
| **Status** | Fixed, see `review.md` |
| **Region** | `battles_view` |
| **Wave** | unscheduled |
| **Found by** | UI-012's width sweep |

## Symptom

At 1440px and 1520px window width, the "Balanced" work-throttle `ComboBox` in the battles list
toolbar overflows the window: rect `[1417.5 87.5]-[1527.2 114.5]` against a content width of 1543px.
It does not overflow at 1280px or 1600px, so it is specific to where the toolbar wraps.

## Cause

The `ComboBox` under-reports its width to the wrapping layout, so the layout places it believing it
fits. Confirmed independent of UI-012: reverting that change and re-rendering gives an identical
rect and identical overflow.

## Notes

Found by the width sweep added for UI-012, not by any fixed-width scene, which is the point of
sweeping. `uitest_toolbar_dividers_keep_content_on_both_sides` already renders these widths, so the
reproduction is free.

Note `checks.rs` does flag horizontally escaped widgets, so a permanent scene at 1440px would fail
the suite on this. Adding one is part of the fix, not a precondition.

## How to verify

Render the battles view at 1440 and 1520 and confirm nothing escapes the panel horizontally, then
add a permanent scene at one of those widths so `uitest_layout` guards it.
