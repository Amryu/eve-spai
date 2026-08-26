# UI-024 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-024-composer-border`
**Caused by:** UI-021, which is mine

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 19.3 min across 1 round, 93 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 30/6 lines (added/removed), excluding the harness |
| **Harness code changed** | 150/20 lines |
| **Suite** | 443 to 445 passing |
| **Follow-ups** | corrected a measurement in UI-021's review |

## The change

The composer draws its own border outside the scrolling region. `composer_frame(ui)` builds an
`egui::Frame` styled like a text edit (`text_edit_bg_color()`, `widgets.inactive.bg_stroke` and
corner radius), `Frame::begin`/`end` wraps the `ScrollArea`, and the `TextEdit` takes
`frame(Frame::NONE)`. `max_height` drops by `COMPOSER_MARGIN.sum().y` since the padding moved out
of the scrolled content.

The inner margin is `COMPOSER_MARGIN - stroke.width`, copying what `TextEdit` does internally, so
the box still measures exactly `COMPOSER_MARGIN.sum().y` more than its content.

## The trap the suggested direction missed

`ScrollArea` inflates its content clip by `visuals.clip_rect_margin` (3px). Invisible while the
border was inside it; with the border outside, line eleven painted 3px **under** the bottom border.
Fixed with `content_ui.visuals_mut().clip_rect_margin = 0.0`, so the galley clips at the box edge.

Worth recording: moving a frame out of a scroll area is not just a nesting change, the clip
behaviour moves with it.

## Focus ring

`frame.frame.stroke` is patched after layout from the field's own response: `selection.stroke` when
focused, `ui.style().interact(&resp).bg_stroke` otherwise. That is egui's own rule for a stock
`TextEdit`, minus the hover `expansion`, deliberately left out so the box does not resize under the
pointer.

Verified three ways rather than asserted: a test pins the focused border to the theme's selection
stroke and checks it differs from idle; a throwaway probe focused a stock `TextEdit` in
`view_settings` and got `Stroke { width: 1.0, color: #3FA9C9 }`; and the focused composer's border
pixels read `(63,169,201)` on all four sides at both window sizes.

## Correction to UI-021's review

UI-021 recorded the 360x260 overflow composer as **83.9px**. That number was a proxy,
`empty.bottom() - over.top()` across two scenes, and it was 2px off because those two scenes'
composers do not share a bottom edge at that size. Measured directly, the old code allocated
**86.0** there, which is what the new code allocates too.

So the box is byte-identical and only my measurement was wrong. The same rounding explains 153.69
against the recorded 153.9 at 520x480; `10 * row_h + 4 = 153.69` is the real figure.

| Scene | UI-021 recorded | actual, both before and after |
|---|---|---|
| `jabber_popout` empty | 33.9 | 33.94 |
| `jabber_popout_drafting` | 49.0 | 49.0 |
| `jabber_popout_wrapped` | 79.0 | 79.0 |
| `jabber_popout_overflow` | 153.9 | 153.69 |
| `jabber_popout_min_overflow` | 83.9 | **86.0** |

`composer_height` needed no adjustment. What did change is the `TextEdit`'s own AccessKit rect: it
is now the text band alone with the margin outside it, so the UI-021 tests moved from
`(rect.height() - 4.0)` to `rect.height()` and gained a direct assertion on the real painted box.

## Teeth, confirmed independently

The border has no AccessKit node, so the tests read `harness.output().shapes` for the stroked
`RectShape` spanning the composer, which gives both its rect and the clip it was painted under.

I reverted `app.rs` to the pre-fix state myself and got:

```
jabber_popout_overflow clips the composer border:
  [[16.0 308.0] - [504.0 522.0]] painted under [[8.0 305.3] - [512.0 465.0]]
```

The border reaches y=522 under a clip ending at y=465, so 57px of it was cut. That is the user's
report stated numerically. All four composer tests fail on the old code.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 443 passed (+32) | **445 passed** (+32) |
| with `--features fc-rescue` | 469 passed | 471 passed |
| `cargo test --bin eve-spai uitest` | 34 passed | 36 passed |

## Screenshots

- `after/jabber_popout_overflow.png` (520x480): a complete rounded rectangle with ten lines inside
  and nothing bleeding past the bottom stroke. The `before/` shot has **no bottom border at all**
  and a half-row of line eleven hanging past where it should end.
- `after/jabber_popout_min_overflow.png` (360x260): complete rectangle with both corners, bottom
  stroke at y=243 against a panel ending at 251.
- `after/jabber_popout_overflow_focused.png`: same geometry with the cyan `#3FA9C9` ring on all
  four sides.

## Merge note

This landed over UI-023 with `git apply -3`, and the conflict boundary truncated the incoming side
mid-function again, exactly as CLAUDE.md warns. Caught by compile error, repaired, and all nine
jabber tests confirmed present and passing afterwards.
