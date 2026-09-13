# UI-063 review cycle

**Status:** Delivered
**Branch:** `feat/ui-063-map-parity`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered; the timer is answered rather than changed, see below |
| **Suite** | 704 to 705, one new |
| **Follow-ups** | the fatigue cap, if the user wants it changed |

## The eleven gaps

All closed. The row menu gained alternatives and Show info; the panel gained the detour note, the
titan jump line and save/load; the map gained waypoint rings, crosses on avoided systems, the
pending-start ring and the crawling dashes it already used for the travel route.

Save and load write the same `SavedMapRoute` store the page writes to, so a route saved on the phone
loads on the desktop and back.

## The reactivation timer

Correct, and here is why it looks stuck. Simulated against the module's own documented rules for a 6 ly
titan jump:

```
jump 1: red 7.0   blue 70
jump 2: red 7.0   blue 300 (capped)
jump 3: red 30.0  blue 300
jump 4: red 30.0  blue 300
```

`cooldown = max(prev_fatigue / 10, 1 + ly)`, capped at 30 minutes. The first two jumps are governed by
`1 + ly`, which is 7; by the third, a tenth of the accumulated fatigue is past 30 and the cap takes
over. **7, 7, 30, 30 is the formula working**, not a bug, and it is not caused by the fatigue cap:
running it with a 30-day cap gives the same red sequence.

What the 300-minute fatigue cap *does* change is the blue figure, which reads 5h from the second jump
onwards where an uncapped run would show 490, then 3430, then 24010 minutes. The module's header cites
300 minutes; the figure usually quoted for EVE is 30 days. **Not changed here**: it is a game constant,
the change would alter every displayed fatigue, and the timer actually asked about is unaffected either
way. Say the word and it is a one-line change.

## Pochven

Excluded from jump planning, alongside Zarzakh, by region rather than by security: it is null sec by
security and the only ways in are the Triglavian gates, so a jump route through it is one nobody can
fly. `pochven_is_never_jumped_through` pins it.

## The save prompt

The page's own dialog. `prompt` blocks the whole tab, looks like a phishing box on a phone, and cannot
say the one thing that matters at that moment, which is the wormhole expiry.

## The console errors

One found and fixed: the drag readout read the node's position after only checking the node for its
*name*, so a geometry reload during a drag left it dereferencing a system that was no longer in the
index, on every pointer move, which is what "many errors while painting" would look like.

Whether that is the one being seen is unconfirmed: the demo does not reproduce it, and the first line
of the actual console output would settle it in one go.
