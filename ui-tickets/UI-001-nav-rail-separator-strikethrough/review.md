# UI-001 review cycle

**Status:** Fixed and verified, after one rejected round
**Wave:** 1
**Worktree:** `wt/ui-001`

## Round 1: rejected

The first patch made the footer anchor to the bottom only while there was room for it, and
otherwise let Settings flow on as the list's last row. That fixed the 560px strikethrough exactly
as the ticket asked, and the agent disclosed that shorter rails would clip the tail.

I rendered the rail at 460px and 500px and looked. At 460px the rail showed Overview through
Characters, and **Jabber and Settings were not drawn at all**. `main.rs:220` sets
`with_min_inner_size([720.0, 460.0])`, so that is a supported window height. Before the change those
rows overlapped but stayed clickable; after it they were gone with no affordance explaining why. The
patch traded a cosmetic bug at 560px for a reachability bug at 460px, so it went back.

The agent had rejected `ScrollArea` on the grounds that reserving the footer's 68px would clip
Jabber in half at 560px. That reasoning was sound for the original layout but no longer held once
its own threshold existed, because at 560px the footer is not reserved. That was the counter-argument
sent back, along with a requirement to add permanent short-rail scenes.

## Round 2: accepted

`app/src/nav.rs` now picks one of three layouts from the height it actually has:

| Condition | Layout |
|---|---|
| `avail >= list_h + foot_h` (~590px up) | Static list, footer pinned bottom-up with its separator. Unchanged from the original. |
| `avail >= list_h + ROW_HEIGHT` (560 to ~590) | Footer stops being bottom-anchored, Settings becomes the list's last row at the normal pitch. |
| shorter | Primary list in a `ScrollArea` capped at `avail - foot_h`; footer keeps its strip so Settings stays pinned and visible while the list scrolls under it. |

The inline literals `4.0 / 10.0 / 8.0` became `ROW_GAP / FOOT_PAD / FOOT_GAP`, so the threshold is
computed from the same numbers the block spends rather than a guess. The body was split into
`primary_items`, `scrolled_items`, `settings_item` and `pinned_footer`, so the footer and the row
loop each exist once.

Two supporting decisions, both justified in comments:

- **`scrolled_items` culls partial rows.** A row half under the pinned footer still claims a
  full-height AccessKit rect, which lands beneath the footer. `uitest_layout` caught exactly that
  and failed with `Lookup <-> Settings` and `Characters <-> Settings` overlaps at 460px. Culling is
  also the honest answer: a row that is not laid out is not a hit target.
- **`ScrollStyle::solid()`, scoped to that branch.** egui's default floating bar is fully
  transparent when dormant, so it was no affordance at all.

## The harness caught a bug in its own fix

Worth recording: the overlap check rejected the first cut of the scroll branch. That is the
assertion tier doing the job the screenshots cannot, and the mirror image of what happened on this
same ticket, where a screenshot caught the original separator bug that the assertions were blind to
(a separator emits no AccessKit node). Both tiers earned their place on one ticket.

## Harness changes

`nav_scene` gained a height parameter. New permanent scenes: `nav_rail_collapsed_short` and
`nav_rail_expanded_short` at 460px, plus `nav_rail_expanded_tall` at 800px, which covers the
anchored branch in the expanded rail that no scene previously exercised.

New test `uitest_nav_rail_short_reaches_every_item`: at 460px it clicks the pinned Settings without
scrolling, asserts Jabber is absent from the tree, sends a real `MouseWheel` event, then clicks
Jabber and asserts the selection. A screenshot cannot prove reachability; this can. It is also the
first use of wheel input in the harness, which closes part of GAP-008.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 409 passed, 2 ignored | **410 passed**, 2 ignored |
| `cargo test --bin eve-spai uitest` | 10 passed | **11 passed** |
| Screenshot scenes | 42 PNGs | 52 PNGs |
| Nav hit targets, 560px | 11 | 11 (unchanged) |
| Nav hit targets, 460px | n/a | 8 visible, tail reachable by scroll |

Height sweep through `checks::inspect` at 420, 460, 470, 480, 500, 520, 540, 555, 559, 560, 561,
575, 585, 600, 628, 640, 800 in both rail widths, 34 renders: no overlaps, no escapes, no degenerate
rects at any of them.

`cargo check --workspace --all-targets --all-features`: only the pre-existing `unused_mut` at
`app/src/intel.rs:5605`.

## Screenshots

- `before/nav_rail_expanded.png`: a hairline runs through the centre of the Jabber row, cutting the
  icon, the word and the amber unread dot. Settings sits directly beneath with no gap.
- `after/nav_rail_expanded.png`: no line anywhere in the item stack. Jabber intact with its dot,
  Settings below it at the same 9px gap as every other pair. Measured from the debug outlines:
  Jabber 465..504, Settings 513..552, gap 9, matching every other pair.
- `after/nav_rail_expanded_short.png` (460px, new): six rows, a blank strip, the separator, and
  Settings pinned and visible at the bottom. I confirmed the scroll affordance by sampling pixels
  rather than by eye: a 6px band at x=182..187 in `(35,39,42)` against the `(11,15,18)` ground.
- The nine `view_*.png` at 800px are unchanged, taking the anchored branch.

## Residual risk

`SEPARATOR_H = 6.0` and the culling `step` encode egui's spacing model rather than reading it from
the style, because egui exposes neither. If egui changes either, the threshold drifts a few pixels
and a row could poke under the footer. The 460px scenes plus `uitest_layout` catch precisely that,
which is why those scenes are permanent rather than throwaway probes.

The scrollbar is present but low-salience against this theme's ground. Reachability is now covered
by a test; discoverability at 460px is a judgement call worth revisiting if anyone reports it.

Between 559px and 560px a blank strip of up to one row height sits between the last full row and
the separator, the cost of not laying out a partial row. It vanishes at 560 when the middle branch
takes over.
