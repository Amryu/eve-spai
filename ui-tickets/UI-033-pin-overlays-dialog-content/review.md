# UI-033 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-033-dialog-pin-strip`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 17.4 min across 1 round, 64 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 23/4 lines (added/removed), excluding the harness |
| **Harness code changed** | 73/0 lines |
| **Suite** | 475 to 476 passing |
| **Follow-ups** | unblocks GAP-010 |

## The width inset was tried first, and measured failing

The ticket listed three options and I would have picked the width inset, since it costs no height.
The agent **built it first** and it fails on the ticket's own worst case.

`jump_bridges_window`'s header is a non-wrapping `ui.horizontal` of label, "?", and hyperlink
needing about 400px. Narrowing the body to 385px pushed the row past its own edge and **the pin still
covered "…gates"**: pin `[[399 8]-[432 35]]` over link `[[295.5 13.5]-[408 28.5]]`. Making that row
`horizontal_wrapped` then split the link's galley across two lines and tripped the text-overlap
pass.

So the inset only works if dialog bodies are rewritten too, and **17 of the 22 dialogs have no
scene**, so the rest could not have been verified. Both experiments were reverted; the final diff
touches no dialog body.

## The chosen fix

`dialog_viewport_ext` opens its body with `ontop_pin_strip(ui, id)`, a frameless `Panel::top` sized
exactly to the pin, drawing it right-aligned. `ontop_pin_w` is factored through a new
`ontop_pin_size` whose x is byte-identical; only the new y is raised to `interact_size.y`.

**The height cost is real: 27px on every `dialog_viewport` dialog**, which range from 320 to 680
tall.

UI-020's review rejected exactly this shape, on the grounds that a reserved row is a large fraction
of the 300x118 dscan window. **That objection does not apply here, and I verified why**:
`dscan_popup` calls `ontop_pin` directly at `app.rs:14752`, not through `dialog_viewport_ext`. So
the small window is untouched and the objection was scoped to a caller this change does not reach.

## Every `ontop_pin` caller

| caller | fate |
|---|---|
| jabber tab bar (`ontop_pin_ui`) | untouched, `ontop_pin_w` returns the same 33px, confirmed by `jabber_popout.png` matching UI-020's result including the "w…" ellipsis |
| map viewport | unchanged floating `Area`, no line in the diff |
| `dscan_popup`, 300x118 | unchanged floating `Area`, no line in the diff |
| `dialog_viewport_ext` | **changed**, this was the bug, ~22 dialogs |

## Measured

| scene | pin before | over | pin after | first content after |
|---|---|---|---|---|
| `dialog_jump_bridges` | `[[401 6]-[434 33]]` | link `[[295.5 13.5]-[408 28.5]]` | `[[399 8]-[432 35]]` | link `[[295.5 40.5]-[408 55.5]]`, clear by 5.5px |
| `dialog_coalitions` | `[[481 6]-[514 33]]` | paragraph `[[8 8]-[507 53]]` | `[[479 8]-[512 35]]` | paragraph `[[8 35]-[507 80]]` |
| `dialog_battle_filter` | `[[541 6]-[574 33]]` | paragraph `[[8 8]-[567 38]]` | `[[539 8]-[572 35]]` | paragraph `[[8 35]-[567 65]]` |

## The new gate, and why it needed to be scene-specific

`uitest_dialog_pin_is_clear_of_the_dialog_body` covers the five scenes routing through
`dialog_viewport_ext`, measuring the pin against every painted `Label` and every other button, then
**clicking it and asserting the toggle flipped**. So "visible and clickable" is measured rather than
assumed, which is what stops this fix trading one bug for another.

`checks.rs` untouched, as instructed. Both halves teeth-checked on real code: against the pre-fix
`ontop_pin(ctx, id)` it reproduced the ticket's three overlaps verbatim, and offsetting the click
point by 200px made all five report "clicking the pin did nothing".

## Screenshots

- `after/dialog_jump_bridges.png`: the pin sits alone in its own row; "Imperium stargates" renders
  fully in link blue on the row below with nothing over it.
- `after/dialog_coalitions.png`: the paragraph reads to its last word with the pin unobstructed.
- `after/dialog_battle_filter.png`: two-line header fully visible, rule cards unchanged.
- `jabber_popout.png`: identical to UI-020's result.

## Unblocks GAP-010

That gap was deliberately sequenced after this ticket, because adding a hit-target-against-text pass
while this bug existed would have failed the eight dialog scenes. It can go ahead now.

## Note

`cargo fmt` was not run: the tree is not rustfmt-clean at HEAD, so it would have rewritten unrelated
files. Worth knowing.
