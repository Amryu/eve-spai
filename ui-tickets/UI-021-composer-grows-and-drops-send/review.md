# UI-021 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-021-composer`
**Depended on:** GAP-004


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 15.3 min across 1 round, 81 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 75/48 lines (added/removed), excluding the harness |
| **Harness code changed** | 153/0 lines |
| **Suite** | 438 to 441 passing |
| **Follow-ups** | UI-024 |

## The ticket had the cause wrong

I wrote that the field was "pinned at `desired_rows(2)`". It was not. `TextEdit` already grows with
its galley (`size.y = galley.y.at_least(min_inner_height)`).

What actually pinned it was `let composer_h = 32.0;` at `app.rs:3858`: the history `ScrollArea`
reserved everything except 32px, so the composer had nowhere to grow into. Changing `desired_rows`
alone would have done nothing visible.

## The change

`composer_h` is now `composer_height(ui, draft, avail_w)`, which lays the draft out with the Body
font at the composer's real wrap width and uses `galley.size().y` directly, clamped to 2..=10 rows
plus the frame margin:

```rust
let galley = ui.ctx().fonts_mut(|f| f.layout(draft.to_owned(), font_id, color, wrap_w));
galley.size().y.clamp(row_h * COMPOSER_MIN_ROWS, row_h * COMPOSER_MAX_ROWS) + COMPOSER_PAD.y
```

No division and no row counting, so a long wrapped line counts as the rows it actually occupies,
which is what the ticket asked for. The composer's own `ScrollArea` now uses `max_height(composer_h)`
rather than a fixed `row_h * 8.0`.

Send is gone, along with the `ui.horizontal_top` wrapper it needed, and `desired_width` is the full
`ui.available_width()` since the `- 60.0` reservation is no longer required.

## One clamp beyond the ticket, correctly added

A 10-row composer is taller than the entire body of a 360x260 popout. `composer_h` is capped at
`(body_h - HISTORY_MIN_H - 8.0)`, otherwise the field ran off the bottom edge.

The agent flagged why that mattered: **`checks.rs` only catches horizontal escape**, so nothing in
the suite would have failed on a field running off the bottom. That is a real blind spot in my
checker, deliberate at the time (scrolled content below the fold is normal, so vertical escape is
ambiguous) but worth recording. Noted on GAP-002.

## Measured, 520x480

| Scene | Draft | Visible height | Rows |
|---|---|---|---|
| `jabber_popout` | empty | 33.9 | 2.00 |
| `jabber_popout_drafting` | 3 newlines | 49.0 | 3.01 |
| `jabber_popout_wrapped` | one long line, no newline | 79.0 | 5.01 |
| `jabber_popout_overflow` | 14 lines | **153.9** | **10.01, capped** |

At 360x260, `jabber_popout_min_overflow` shows 83.9 (5.34 rows), the small-window clamp biting
before the 10-row cap.

The AccessKit rect keeps growing past the cap (214.0) because that is the height the field *wants*
inside its scroll area. The visible band is the number that matters and both tests assert on it.

## Enter and Shift+Enter, driven not reasoned

`uitest_jabber_composer_enter_sends_shift_enter_wraps` asserts the Send button is gone
(`query_by_label("Send")` is `None`), then focuses the field, sends Shift+Enter and checks the draft
gained a newline and kept its text, then sends Enter and checks the draft is empty.

Headless has no `jabber_tx`, so the only thing that can empty the draft is the `std::mem::take` in
the send branch. An empty draft is therefore proof the send fired.

**I confirmed it has teeth** by disabling the send branch: it fails. Restored, it passes. That
mattered more than usual here, since removing a button that was one of two ways to send is exactly
the change that could silently leave no way at all.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 438 passed, 2 ignored (+32) | **441 passed**, 2 ignored (+32) |
| with `--features fc-rescue` | 464 passed | 467 passed |
| `cargo test --bin eve-spai uitest` | 29 passed | 32 passed |

Three new permanent scenes: `jabber_popout_wrapped`, `jabber_popout_overflow`,
`jabber_popout_min_overflow`. `cargo check --workspace --all-targets --all-features` is at the single
pre-existing warning; the agent also fixed a deprecation it hit on the way (`Context::style` to
`global_style` in a test helper).

## Screenshots

- `after/jabber_popout.png`: no Send button, hint text runs the full width, field at 2 rows.
- `after/jabber_popout_drafting.png`: all three lines visible. Previously the third was cut in half
  at the field's bottom edge.
- `after/jabber_popout_wrapped.png`: one unbroken sentence occupying 5 rows, fully shown, which is
  the case `desired_rows` alone would have got wrong.
- `after/jabber_popout_overflow.png`: ten rows visible plus a sliver of the eleventh, lines twelve
  to fourteen below the fold. The census confirms a second `ScrollBar` node in the overflow scenes,
  so the composer really does have its own bar; egui's floating bar fades when idle so it does not
  paint in a static shot.
- `after/jabber_popout_min_overflow.png`: composer capped short of 10 rows, history keeps two
  message rows, nothing runs off the bottom.

## Out of scope, untouched

The delve911 rescue reply box keeps its own Send button, now at `app.rs:5519`. Different feature,
different widget, and it is a singleline field.
