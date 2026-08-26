# UI-002 review cycle

**Status:** Fixed and verified
**Wave:** 2 (paired with UI-001 on `nav.rs`, no region overlap)
**Worktree:** `wt/ui-002`, seeded from the main tree including the UI-003 fix


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 5.4 min across 1 round, 34 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 7/6 lines (added/removed), excluding the harness |
| **Harness code changed** | 0/0 lines |
| **Suite** | 409 to 409 passing |
| **Follow-ups** | none |

## The change

`app/src/app.rs`, `intel_row`. 7 insertions, 6 deletions, one region.

```rust
if let Some(j) = from_you {
    let jtxt = if j == 0 { "here".to_owned() } else { format!("{j}j") };
    // Padded to 4 so "here", "3j" and "12j" share one column down a stack of cards.
    ui.label(egui::RichText::new(format!("{jtxt:>4}")).monospace().color(jumps_color));
}
```

The `None` arm now emits no widget. The `{:>4}` padding stays on the `Some` path, because that is
what keeps the values in one column across stacked cards.

## Review

The interesting question was whether the padding could be dropped along with the empty label. It
could not: the alert window stacks several cards, and `{:>4}` inside a monospace run is what aligns
"3j" against "12j" against "here". Deleting it would have traded a dead widget for a ragged column.

The agent checked whether mixed lists are real rather than assuming. They are:
`from_you` derives from `r.primary_system()`, which is `self.systems.first()`, so any report with
no parsed system yields `None` while its neighbours yield `Some`. It accepted the resulting ~39px
shift for `None` cards on the grounds that a card with no parsed system has no system chip to align
anyway. That reasoning holds, and the alternative (reserving invisible width) is the bug itself.

The shape now matches `from_you_chip` at `app.rs:19277`, which already handled `None` correctly.
Reuse was rejected because that helper hardcodes `.weak()` while `intel_row` colours the text
`standing::CORP`, so calling it would have silently changed the colour.

**Follow-up worth doing, not blocking:** `from_you_chip` has exactly one caller and is now a
near-duplicate of this block. Parameterising it on colour and calling it from both places would
centralise the `None` handling so this cannot come back a third time. Left alone here because it
sits outside the ticket's region.

## Correction to the ticket

The ticket implied the census would show this widget disappearing. It does not, and cannot: the
dead label reports as AccessKit `Label`, and `checks.rs` filters hit targets on an allowlist of
interactive roles that deliberately excludes `Label`. Hit-target counts are unchanged at 6 / 2 / 34
/ 34 across the four intel scenes.

This is direct evidence for GAP-008's note about the role allowlist. A click-sensed `Label` is a
real widget that the checker cannot see. The census did register the change two other ways: the
role histogram drops one `Label` and one `TextRun` per intel card, and the leftmost chip moves.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 409 passed, 2 ignored (+32) | 409 passed, 2 ignored (+32) |
| Intel scene hit targets | 6 / 2 / 34 / 34 | 6 / 2 / 34 / 34 (see above) |
| Role histogram, intel card | Label:4 TextRun:4 | Label:3 TextRun:3 |
| First chip origin, typical | x=140.1 | x=100.8 |
| Layout assertions | clean | clean |

`cargo check --workspace --all-targets --all-features`: only the pre-existing `unused_mut` at
`app/src/intel.rs:5605`. No new warnings.

## Screenshots

- `intel_row_typical.debug.png`: before, the overlay outlines an empty box between the "45s" age
  text and the first system chip. After, the box is gone and every chip on the row moves left by
  about 39px. Same wrap, no new row.
- `intel_row_torture_narrow.png` (320px): before, row one held only the 7-K5EL chip with dead space
  beside it, and 1DQ1-A wrapped to row two. After, 7-K5EL and 1DQ1-A share row one. The reclaimed
  width is visible and buys a chip per row.
- `alert_window_torture.png` (three stacked cards): each card's first chip moves from x~150 to
  x~103 and remains at the same x across all three, so the chip column holds. The right-aligned
  `{:>7}` timestamp column is untouched and still lines up.

## Residual risk

Cards with `from_you == None` now sit ~39px further left than cards with a value. That is the
correct trade, but if a future design wants the two aligned, the fix is a leading spacer on the
chip run, not a re-introduced empty label.
