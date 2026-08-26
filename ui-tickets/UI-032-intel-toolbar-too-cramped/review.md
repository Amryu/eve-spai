# UI-032 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-032-compact-intel-toolbar`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 20.5 min across 1 round, 60 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 35/14 lines (added/removed), excluding the harness |
| **Harness code changed** | 103/21 lines |
| **Controls at 1280px** | **1135px to 888px** |
| **Search field** | **57px to 296px**, 5.2x |
| **Suite** | 473 to 474 passing |

## The dropdown was declined, with evidence

The user suggested it and it was the biggest single win at ~160px. The agent measured and did not
need it: the other cuts alone take the row to 888px, leaving 296px for the search field. A
`toolbar_combo` would take a further 170px the row no longer needs, in exchange for **putting a
click in front of the control this view exists to flip**.

The nod to the idea is real but cheap: the five chips are now grouped at 2px instead of 8px spacing,
so they read as one segmented control. 24px recovered at zero interaction cost.

That is the right call, and it is the right call *because it was measured*. Doing what was suggested
without checking would have cost a click for width that was already available elsewhere.

## Widths

| control | before | after |
|---|---|---|
| five type chips plus gaps | 273.2 | 249.2 |
| `≤ jumps` label + spinner | 85.3 | 50.5, one control |
| bridge toggle | 129.8 | 93.5 |
| `outdated after` label + spinner | 133.0 | 66.5, one control |
| severity | 79.3 | 33.0, icon |
| zKill range | 201.2 | 172.3 |
| **controls** | **1135** | **888** |
| **search field** | **56.9** | **295.8** |

## The two that had to survive, and did

**UI-004's grammar.** Zero now reads `within the feed's range`, still a prepositional phrase, never
a bare noun. `kill_intel_range` is shared, so the alerts row moved with it and the two stay
structurally identical.

**UI-025's discoverability.** Still a ticked checkbox in the toolbar, in the same group as the jumps
spinner, tooltip verbatim. Only the word "count" is gone.
`uitest_intel_toolbar_carries_the_bridge_toggle` still asserts role `CheckBox` plus a label, so
folding it into an icon or a menu would fail that test. The constraint is enforced, not just
honoured.

## A pre-existing bug the work exposed

Switching to `toolbar` made the row wrap, which revealed that **the toolbar ran 511px past a 720px
window**. It had been invisible because there was no intel scene at the minimum window width. Also
fixed: `available_width()` in a wrapping row returns the whole row, so the search field was asking
for 1192px and always wrapped onto its own line. Same class as UI-017's ComboBox.

A permanent `view_intel_narrow` at 720px now exists, and the divider sweep runs Intel alongside
Battles across 720 to 1600.

## The width budget

`uitest_intel_toolbar_leaves_room_for_the_search_field`: controls at most 75% of the window, the
field still on the toolbar's row, and the field at least as wide as its own hint text laid out in
the current font.

**75% was measured, not picked round.** Restoring each ticket's copy in turn: the row was 854px
before either, 1008px after UI-004, 1146px after UI-025. A 960px ceiling fails both and passes the
row they were added to. Current 888px leaves 72px of headroom, enough for a normal label but not
another 130px checkbox.

**I verified the guard myself.** My first attempt reverted only the two copy strings, which adds
about 65px and lands at 953, just under the ceiling, so it passed. That is consistent with the
agent's numbers rather than a toothless test. Injecting a wide label instead fires it:
`the intel toolbar's controls take 1157px of a 1280px window, over the 960px budget`.

## Screenshots

- `after/view_intel.png` at 1280: one row,
  `All Hostile Clear Kill Threat | ≤ any [ ] jump bridges | ⏱ 300s | palette | [x] zKill intel
  ⇅ within the feed's range | search`, with the field wide enough to show its whole hint. Dividers
  between groups, none at a row edge.
- `after/view_intel_narrow.png` at 720: two rows, row 1 ending at the palette with its trailing
  divider correctly dropped, row 2 taking the field full width. Nothing clipped or past the panel.
- `after/view_intel_feed.png`: the `2j` chip still renders as UI-025 left it.

Both new icons were grepped in egui-phosphor before use and render as real glyphs.
