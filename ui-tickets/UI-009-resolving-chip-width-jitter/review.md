# UI-009 review cycle

**Status:** Fixed and verified
**Wave:** 4 (paired with UI-007 on the alert viewport callback, no region overlap)
**Branch:** `fix/ui-009-resolving-chip-width`


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 12.5 min across 1 round, 55 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 16/3 lines (added/removed), excluding the harness |
| **Harness code changed** | 165/0 lines |
| **Suite** | 412 to 414 passing |
| **Follow-ups** | found a dead tooltip |

## The change

The dots move into a fixed-size atom slot inside the button, sized once from the widest phase:

```rust
let slot = ui.painter().layout_no_wrap("...".to_owned(), font, Color32::PLACEHOLDER).size().x;
let dots = egui::RichText::new(".".repeat(phase))
    .weak()
    .atom_size(egui::vec2(slot, row_h))
    .atom_align(egui::Align2::LEFT_CENTER);
```

Left alignment means the visible dots grow rightward into reserved space, so neither the chip nor
anything after it moves. The 450ms repaint is unchanged: the animation was never the problem, the
width was.

## Review

**Rejecting `egui::Spinner` was the right call, for a reason I checked.** A spinner is the obvious
answer: constant square, built in, already used three times in this app. But `Spinner::paint_at`
calls `ui.request_repaint()` unconditionally (`egui/src/widgets/spinner.rs:40`), taking this chip
from 2fps to the display rate. That would be fine for a transient state and this one is not always
transient: `pilot.rs` records a permanent negative for names that can never be characters, and its
own comment says such a name would otherwise be left "stuck on the `...` animation". A permanently
stuck chip pinning the app at 60fps is a worse trade than jitter.

`Button::min_size` was also rejected correctly: it would hold the chip width but centre the
content, so the dots would still shuffle inside a fixed box.

## Second bug found on the way

The chip's tooltip never worked. `on_hover_text` routes through `Tooltip::for_enabled`, which gates
on `response.enabled()` (`egui/src/containers/tooltip.rs:75`), and this button is
`add_enabled(false, ..)`. So the only affordance explaining what the chip meant was dead. Now
`on_disabled_hover_text`, with a test that fails when reverted.

I verified this in the egui source rather than taking it on trust.

## Proof the width is stable

`uitest_intel_row_resolving_chip_holds_its_width` renders the row at three consecutive `now` values
(phase is `now * 2 % 3`, so consecutive seconds cover all three), pulls the chip rect from the
AccessKit tree, and asserts all three are equal, plus that the three labels genuinely differed, so a
passing test cannot be vacuous.

| phase | before | after |
|---|---|---|
| `.` | `[17.0 47.0] - [56.2 75.0]`, w 39.2 | `[17.0 47.0] - [63.6 75.0]`, w 46.6 |
| `..` | `[17.0 47.0] - [59.4 75.0]`, w 42.4 | `[17.0 47.0] - [63.6 75.0]`, w 46.6 |
| `...` | `[17.0 47.0] - [62.6 75.0]`, w 45.6 | `[17.0 47.0] - [63.6 75.0]`, w 46.6 |

3.2px of movement per dot, now zero.

**I confirmed both new tests fail on the unfixed code**, by stashing the `app.rs` hunk:
`uitest_intel_row_resolving_chip_holds_its_width` fails on the rect comparison, and
`uitest_intel_row_resolving_chip_explains_itself_on_hover` fails with "hovering the resolving chip
at [38.2 61.0] showed no tooltip".

## Fixture and scene added

`fixtures::intel_resolving(clock)` builds a card with one resolved pilot and one unresolved, which
is the only state that draws the chip. It takes the clock and sets `received = clock - 20` so the
age chip reads identically at every phase and cannot itself reflow the row.

Scene `intel_row_resolving_phases` draws that card three times, once per phase, stacked.

Worth recording: **the existing `intel_row_torture` scene never rendered this chip at all**, because
every torture pilot name is in `resolved_pilots`. So `before/intel_row_torture.png` does not show
the bug, and its re-render is pixel-unchanged. The ticket's screenshot was the wrong evidence, and
the agent said so rather than quietly rendering something that looked close enough.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 412 passed, 2 ignored (+32) | **414 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 13 passed | 15 passed |
| Chip width across phases | 39.2 / 42.4 / 45.6 | 46.6 / 46.6 / 46.6 |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Screenshots

`after/intel_row_resolving_phases.png`: three identical cards. On each card's second line the
leftmost chip is the person icon followed by one, two, then three weak dots. The chip's border
starts and ends at the same x in all three, with visible empty room to the right of the single dot
where the reserved slot sits. The "· Scout Charlie · delve.imperium" footer starts at the same x in
all three rows, so nothing downstream moves. The `.debug.png` shows the hit rect is the same size in
all three.
