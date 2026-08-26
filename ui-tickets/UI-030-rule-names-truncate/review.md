# UI-030 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-030-rule-name-width`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 14.7 min across 1 round, 65 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 36/46 lines (added/removed), excluding the harness |
| **Harness code changed** | 66/3 lines |
| **Suite** | 474 to 475 passing |
| **Follow-ups** | none |

## The change, and it is an interaction change

The two reorder arrows move out of **every rule row** into a single non-resizable
`Panel::bottom` under the list, acting on the **selected** rule and disabled at the ends. The name
button drops the `fit_chars`/`truncate_to` character-count guess for egui's own
`Button::selectable(..).truncate()`. Help text updated to match: "Drag a rule's handle to reorder
it, or select it and use the arrows under the list."

**This is worth flagging to the user as a behaviour change**, not just a layout fix: reordering is
now select-then-click rather than click-the-arrow-on-that-row.

The justification holds up. The arrows were spending 82px of a ~165px row on a **secondary** path to
something drag already does, per the panel's own help text. In a footer they cost about 35px of
vertical height once for the whole list rather than 82px horizontally per rule, and they become
better targets in the process: 27px tall instead of `small_button`'s 17px.

## Rejected, with reasons

- **Widen the default panel:** buys ~8 chars per 60px, still truncates "Hostiles near home" at 300px,
  costs editor width permanently, and leaves the 180px overlap untouched.
- **Hover-reveal in the row:** either the reserve stays, buying nothing, or the name re-truncates as
  the pointer crosses each row.
- **Per-rule second row:** doubles the height of every row in a list built to be scanned.
- **Middle truncation or a tooltip:** neither returns a pixel of width. The tooltip is kept anyway,
  and is what carries the still-truncated 180px case.

## Characters visible

| rule | before, 240px | after, 240px | after, 180px |
|---|---|---|---|
| Hostiles near home (18) | 7 + ellipsis | **18, full** | 12 + ellipsis |
| Cyno in Delve (13) | 7 + ellipsis | **13, full** | 13, full |
| Quiet hours (11) | 7 + ellipsis | **11, full** | 11, full |

The AccessKit label is now the full name rather than a pre-truncated string, so screen readers get
the whole name at any width. That is a real accessibility improvement that fell out of dropping the
manual truncation.

## The overlap cannot recur

Names occupy y 155..254; the arrows are at y 713..740 in a different panel. UI-019's overlap
(`Cyno in Delve <-> ARROW_DOWN`) is structurally impossible while the arrows are out of the row.
`uitest_layout` green on both alert-rule scenes.

## The 180px minimum is now safe

This is the second half of the fix and the ticket's open question. The `.max(40.0)` floor and the
whole `reorder_w` subtraction are gone, and `truncate()` sizes the galley against the real
remaining width, so the button can never exceed its space at any width. Measured at 180px:
`"Hostiles near home" [[137.0 155.0]-[238.9 182.0]]`, inside the panel and clear of everything.

A long name is still cut at 180px, which is unavoidable in 180px; the hover tooltip carries the rest.

## New scene

`view_alert_rules_narrow` pins the left panel to **180.0**, the minimum a user can drag to. The
harness could not reach that before, because a `Panel` takes its width from persisted state and
there is no pointer to drag the separator; the scene writes `egui::PanelState` for
`alert_rules_split` before each frame. That closes the gap the ticket's own Note called out.

## An honest note on the new test

`uitest_alert_rule_names_fit_and_clear_the_arrows` has two halves. The width half has teeth,
confirmed by restoring the 82px reserve and dropping `.truncate()`. The intersection half
**cannot fail by construction** now that the arrows live in a different panel; the agent said so
rather than presenting it as a guard. It documents the invariant, and `uitest_layout` remains the
real gate for overlap, as it was for UI-019.

## Screenshot

`after/view_alert_rules.png`: all three names render complete with no ellipsis anywhere in the list,
"Hostiles near home" highlighted as the selection. The footer sits at the bottom of the rule panel
above the status bar with a separator over it, up arrow greyed out because rule 0 is selected, down
arrow live.

`after/view_alert_rules_narrow.png` at 180px: "Hostiles nea…" with the other two intact.
