# WEB-060 &mdash; The titan repositions for its own convenience, and its jump runs backwards

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/map.js`, `app.rs` |
| **Reported by** | user |

## Symptoms

1. "The titan repositioning does not seem to work properly (or it just doesn't provide much benefit?).
   It needs to save at least 1 jump over not repositioning it. The repositioning seems to only jump
   the titan the fewest jumps possible. However the titan should jump so the gate route is as small as
   possible."
2. "The titan jump indicator on the map is animated the wrong way. It also should not share a color
   with the regular jump animated line"

## Cause

1. The landings were sorted by how far the titan had to jump and then cut to the nearest twelve, so
   the search only ever looked at the systems next door. That optimises the titan's convenience, which
   is not what anybody is asking. The fleet's gate count was checked afterwards, against candidates
   chosen for the wrong reason.

2. A positive `lineDashOffset` runs the dashes backwards, so the titan appeared to be jumping to where
   it already was, in the same colour as a capital jump.

## How to verify

A start whose neighbours are all equally useless and a good bridge point eight gates out. The old
search cannot find it; a correct one has to.
