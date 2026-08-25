# UI-012 review cycle

**Status:** Fixed and verified
**Wave:** 5 (paired with UI-011 on `intel_row`, no region overlap)
**Branch:** `fix/ui-012-toolbar-dividers`

## The change

Fixed in the helper, not the call site, which is right for two reasons: a wrapping toolbar's break
point moves with the window so the call site cannot know where it lands, and `toolbar_sep` is shared
by both battle toolbars.

`toolbar_sep` no longer paints. It skips entirely when it is already at a row start or would wrap
onto one, and otherwise reserves its 10px, adds a `Shape::Noop` placeholder, and records the
placeholder index with its rect and row top. A new `toolbar(ui, |ui| ..)` wrapper owns the
`horizontal_wrapped` and resolves the queue at the end of the row, replacing a placeholder with a
real vline only if some content end was recorded on the same row past the divider's right edge.

## Is the machinery justified?

It is more apparatus than a cosmetic divider suggests, so I checked whether something simpler works.
It does not. Suppressing a divider at a row *start* is easy, but the reported bug is a divider at a
row *end*, and in a wrapping flow nothing at emit time knows whether the row continues. That needs
either lookahead or deferral. `painter.add(Shape::Noop)` then `painter.set(idx, ..)` is egui's own
documented pattern for exactly this, so the shape of the fix is idiomatic rather than inventive.

One detail worth keeping: `ui.available_width()` is useless here, because a wrapping layout reports
the full row width. The wrap test uses `available_rect_before_wrap().width()`, which mirrors egui's
own condition in `Layout::next_frame`.

## Callers checked

All 12 `toolbar_sep` calls sit in two `horizontal_wrapped` blocks, both in `battles_view`.

- **Battles list toolbar**: rendered at 760, 820, 900, 980, 1060, 1140, 1200, 1280, 1360, 1440, 1520
  and 1600. At 1280, the ticket's case, row 1 ends on "My shared BRs" and row 2 begins flush with the
  zKill field. Every width is clean at both edges and every mid-row divider is still drawn.
- **Battle detail toolbar**: not reachable headlessly before, since the brview worker never runs. The
  agent seeded the selection and detail cache to reach it, and swept 720 to 1600.

## Test added

`uitest_toolbar_dividers_keep_content_on_both_sides` sweeps 720 to 1600 in 40px steps and requires
every painted divider to have a content rect on its own row to both left and right.

Dividers emit no AccessKit node, so the harness cannot see one at all. A `#[cfg(test)]`-only
recorder in `toolbar` publishes the painted rects per pass, and the assertion cross-checks them
against the AccessKit tree, so it is independent of the drawing logic rather than asserting the
implementation against itself.

**Teeth confirmed, on the second attempt.** My first check stashed all of `app.rs` and the test
passed, which I briefly took as the test being toothless. It was my check that was wrong: stashing
`app.rs` also removes the recorder, so `painted_toolbar_seps` returns empty and the assertion has
nothing to examine. Reverting only the row-edge suppression while keeping the recorder fails with
`divider at [[72.0 154.2] - [82.0 180.2]] starts a row at 800px`, exactly as reported.

Worth generalising: a test that reads a `#[cfg(test)]` hook cannot be teeth-checked by reverting the
whole file, because the hook goes with it.

## Scenes kept

- `view_battles_narrow` (720x800), permanent.
- `view_battle_detail_narrow` (720x800) plus its `battle_detail_scene` builder, permanent. **First
  harness coverage of the battle detail view at all.**

Reaching the detail toolbar needed `battles`, `battle_selected` and `battle_detail_cache` widened to
`pub(crate)`. That is scope creep, accepted: it buys a view that previously had zero coverage, and it
is the same kind of visibility opening GAP-001 and GAP-002 will need anyway.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 415 passed, 2 ignored (+32) | **416 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 16 passed | 17 passed |
| Scenes | 26 | 28 |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Separate bug found, ticketed as UI-017

At 1440px and 1520px the "Balanced" work-throttle `ComboBox` overflows the window, rect
`[1417.5 87.5]-[1527.2 114.5]` against a 1543px content width. The agent verified this is unrelated
by reverting its own change and re-rendering: identical rect, identical overflow. A ComboBox
under-reporting its width to the wrapping layout. Not fixed here.
