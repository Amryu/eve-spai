# UI-024 &mdash; Composer scrolls its own border instead of its contents

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Fixed, see `review.md` |
| **Region** | the composer row in `jabber_window_body` |
| **Reported by** | user |
| **Caused by** | UI-021 |

## Symptom

Once a draft passes 10 rows the composer scrolls, but the `ScrollArea` wraps the whole `TextEdit`
including its frame, so the **border scrolls with the text** and gets clipped at the top and bottom
of the scroll viewport. The box should hold still and only its contents should move.

## Cause, and whose fault it is

Mine. UI-021 kept the pre-existing shape, a `ScrollArea` wrapping the `TextEdit`, and only changed
the height it is capped at:

```rust
egui::ScrollArea::vertical()
    .id_salt("composer")
    .max_height(composer_h)
    .show(ui, |ui| ui.add(egui::TextEdit::multiline(..)))
```

Making the field grow to 10 rows is exactly what made the clipped border visible, since before UI-021
the field was pinned at 32px and rarely scrolled at all.

## Suggested direction, not binding

Draw the frame once, outside the scrolling region, and let the text scroll inside it: an
`egui::Frame` styled like a text edit, containing the `ScrollArea`, containing a
`TextEdit::multiline(..).frame(false)`. Then the border is a fixed box and only the galley moves.

Check the focus ring still appears on the outer frame, since `frame(false)` drops the `TextEdit`'s
own focus styling. A focused composer must still look focused.

## How to verify

`jabber_popout_overflow` (520x480, a 14-line draft) and `jabber_popout_min_overflow` (360x260)
already render the scrolling state. The border must be a complete rounded rectangle in both, not
clipped at the top or bottom edge. Compare against `before/jabber_popout_overflow.png`.

Keep the UI-021 assertions green: `uitest_jabber_composer_grows_then_caps`,
`uitest_jabber_composer_yields_to_a_small_window` and
`uitest_jabber_composer_enter_sends_shift_enter_wraps`.
