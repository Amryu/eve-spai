# UI-026 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-026-bridge-indicator`
**Depended on:** UI-025

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 71.6 min across 1 round, 139 tool calls, plus a worktree clobbering and recovery |
| **Patches rejected on review** | 0 |
| **App code changed** | 171/34 lines (added/removed), excluding the harness |
| **Harness code changed** | 394/4 lines |
| **Suite** | 458 to 464 passing |
| **Follow-ups** | UI-029, the overlay cannot show the flag |

## The three cases, decided and justified

| case | behaviour |
|---|---|
| setting ON, bridge shortens the route | flagged. `JumpVia::BridgeShorter(gate_jumps)`, carrying the gate answer so the tooltip can name it |
| setting OFF, a bridge would shorten it | **not** flagged |
| gates cannot reach at all | flagged differently: `JumpVia::BridgeOnly`, reading `bridge only` beside the glyph |

The middle decision is the interesting one, and the reasoning is sound: the number on screen is
already the gate answer so nothing about it misleads, the mark's whole meaning is "this is not what a
hostile faces", and in a home region with a bridge network it would land on nearly every card and
stop meaning anything. `jump_via` returns early on `!use_bridges`, so **the gate-only default costs
exactly what it did before**.

The third case is distinguishable without hovering, which matters: "1j via bridge, 2j by gate" and
"1j via bridge, no gate route at all" are materially different and should not render identically.

## Cost, which was not negligible

The naive two-BFS version measured **80 ms per 250-card frame**. The agent measured rather than
assumed, found that, and did both things the ticket permitted:

- **Bounded the second walk.** Gates can never beat the bridged graph, so `jumps_gates_only` capped
  at the number already shown either matches it or proves a bridge is load-bearing. The full-cap
  scan now runs only for genuinely bridge-dependent cards, and only to fetch the tooltip's figure.
- **Memoized per target**, keyed on the graph `Arc`, the player's system and the setting. Keyed on
  the `Arc` rather than an address, so a rebuilt graph cannot hit a stale entry.

**I reproduced the benchmark in release:**

| feed | distances alone | detection cold | detection warm |
|---|---|---|---|
| home, 250 cards, 106 bridge-dependent | 21 us/card | +5 us/card | ~0 |
| map-wide, 250 cards, 199 bridge-dependent | 171 us/card | +336 us/card | 2 us/card |

## Column alignment, the UI-002 constraint

The number keeps its `{jtxt:>4}` monospace label untouched. The mark is a **separate label emitted
only when there is one**, so an unbridged card allocates nothing and sits exactly where it did.

Two tests hold it, both teeth-checked by the agent:
- `uitest_bridge_mark_only_takes_width_on_the_card_that_earned_it`: a reserved spacer makes the gap
  29px, which is the UI-002 shape exactly.
- `uitest_bridge_mark_holds_the_jump_column`: moving the mark in front gives 185 / 164 / 253.

## Colour, icon, tooltip

**`theme::standing::ALLIANCE` (0x9B6FD8).** Jump bridges are alliance infrastructure, and the row
already spends green on cleared reports and amber and red on threat. The agent rejected the map's
bridge green because it sits within a few points of the `clear` green used in the same row, which I
would not have caught.

`ARROWS_LEFT_RIGHT`, confirmed present in egui-phosphor 0.12.0, matching the map overlay's
vocabulary.

> 1j counts your jump bridges, 2j by gate. A hostile can't use your bridges, so 2j is how far away
> they really are.

> Only your jump bridges reach this system, there is no gate route within 50 jumps. A hostile can't
> get here the way you would.

Both are on the number and the glyph, and compact mode routes through the `tip` out-param like the
row's other tooltips.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 458 passed, 4 ignored (+32) | **464 passed**, 5 ignored (+32) |
| with `--features fc-rescue` | 484 passed | 487 passed |
| `cargo test --bin eve-spai uitest` | 45 passed | 51 passed |

## Screenshots

`after/view_intel_feed_bridged.png`, three cards with the setting on: 7-K5EL reads purple `1j ⇆`,
319-3D reads blue `1j` with no mark, Jita reads purple `1j ⇆ bridge only`. All three numbers sit at
one x and only the marked cards' chips shift right. The glyph renders as real arrows.

`after/view_intel_feed.png` at the gate-only default is identical to UI-025's after shot, which is
the evidence the default path is untouched.

## Residual risk

**One slow frame after a player jump on a map-wide feed.** The memo is keyed on the player's system,
so a jump invalidates it: 250 cards at 336 us cold is roughly 84 ms for that single frame. Home feeds
are unaffected at 5 us/card. Acceptable, and worth knowing if anyone reports a hitch on undock.

## Recovered from a clobbered worktree

This agent's worktree was destroyed mid-run by another agent's `git stash`, and its own recovery
attempts were denied six times by the safety classifier, correctly, since every route discarded
uncommitted work. Recovered by capturing both agents' work as patches and applying into a fresh
worktree. The agent confirmed the rescued state was complete rather than assuming it, then built the
memo, the bench and the stronger alignment tests on top.
