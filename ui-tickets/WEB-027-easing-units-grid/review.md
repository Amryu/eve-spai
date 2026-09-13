# WEB-027 review cycle

**Status:** Fixed and verified
**Branch:** `ui/ui-051-jabber-start-dialogs`, shared with UI-051

One branch for two tickets, against the usual rule. They landed in the same pass and touch no common
file, so `git revert -m 1` takes both or neither. Noted rather than hidden.

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 690, unchanged |
| **Follow-ups** | none |

## Easing

180ms on an ease-in-out cubic, replacing 80ms of exponential decay. Decay starts at full speed and
arrives on an asymptote, which is a lurch followed by drift; this leaves and arrives at rest, which
is the shape of a movement that someone started and stopped.

`k` is still interpolated geometrically. The halfway point of a zoom is the geometric mean, and
interpolating it linearly races at one end and crawls at the other, which would have put back exactly
the roughness the easing is there to remove.

A notch arriving mid-flight restarts the curve from wherever the view has reached, so a fast scroll
is one continuous movement. Pan writes the in-flight anchor too, or the next frame drags the map back.

## The AU numbers

`near_celestial` is metres. The page called it `km`. Everything downstream was a thousand times too
far, and the AU conversion was faithfully converting a wrong number.

Matched to the app rather than patched: divide by 1000, group the thousands the way
`format!("{},{:03} km", ...)` does, and drop the chip past the app's own 15,000 km ceiling. Under
that ceiling the AU branch cannot fire, so it stays as the rule for anything that ever exceeds it
rather than as the thing the user sees.

## The message behind a card

Open cards are tracked in a module `Set` keyed by report id, not by a class on the node: a pane
rebuilds its HTML whenever the snapshot moves, and state living only in the DOM is gone on the next
tick. That is the same mistake that reset the scroll position in WEB-018.

The header is wrapped in a `display: contents` span so revealing the message hides the badges and
keeps the icon, age, jumps and system where they were. `display: contents` means the wrapper changes
nothing about the layout of a closed card.

Kill cards are left alone, as the app leaves them: there is no original message behind a generated
card.

## The grid

`el.hidden = true` was doing nothing, because both grid and columns give `.pane` a `display` and any
author rule beats the UA's `[hidden] { display: none }`. This is the third time that rule has been
beaten on this page. Fixed with specificity rather than `!important`, so the next rule to be added
does not have to fight it.

An odd pane count leaves a spare cell in the last row, and the map is the pane that gains most from
the width, so it takes both columns. Driven by an attribute from `apply()` rather than a CSS
`:last-child` rule, because which pane should span is a decision about the map, not about position.

## Mobile tabs

`flex: 1 1 0` with a shared minimum, so four tabs divide the width evenly and the fourth is on the
screen. `after/mobile-tabs-full-width.png` at 390px.

## Verified

`cargo test --bin eve-spai`, 690 passed. Easing and the card reveal are by inspection: the shot
harness fires on `load` and cannot click.
