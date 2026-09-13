# WEB-017 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-017-map-layers`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 667 to 673 |
| **Follow-ups** | none |

## The churn was a hash that could not compare equal

This is the one worth reading. The revision mechanism from WEB-002 was correct and had a test, and it
had never once worked in the running app.

`hash_of` serializes with `serde_json`. A `HashMap` serializes in iteration order, and every instance
gets its own hash seed, so two maps with identical contents serialize their keys in different orders.
The publisher rebuilds `resolved_pilots` via `display_ids` and `last_ship` via `build_last_ship` every
tick. New map, new seed, new order, new hash. Nothing ever compared equal, so every pane published on
every tick, and the page rebuilt every pane and threw away the reader's scroll position.

The fixture test passed throughout, twice over: the fixtures resolve no pilots, so the maps were
empty, and empty maps hash stably. My first attempt at a regression test *also* passed with the bug
reintroduced, because it seeded `status`, which is **cloned** from one long-lived map rather than
rebuilt, and a clone keeps its parent's layout. Both versions of the test were measuring the wrong
thing.

What actually proved it was the running app: 70 publishes in 12 seconds with nothing happening. The
mechanism is pinned by `a_hashmap_would_not_have_hashed_stably`, which asserts the instability
directly and says in its message what to do if it ever stops being true.

The lookups are `BTreeMap` now.

## The map

**Mirrored.** `map::project` uses `center.y - (z - mid_z)`, so screen y runs opposite to z, and the
app has its own test saying north is up. The web build emitted z unflipped and drew the galaxy upside
down. `greater_z_is_higher_on_screen` now says the same thing on this side.

**Canvas, not SVG.** One element per system meant 5255 nodes re-rasterising on every viewBox change,
which is what made zoom feel bad. Canvas draws the same 5255 dots in a millisecond or two, redraws
per frame, and touches no DOM. It also fixes the sizing complaint by construction: everything is
drawn in screen pixels, so a dot is the same size at any zoom instead of growing with the map.

The cost is hit testing, which is now a nearest-node search. For 5255 nodes that is cheaper than
asking the browser to maintain 5255 hit targets.

**Layers.** Bridges and Jove observatories ride with the geometry, since they are static.
Sovereignty, gate camps, scanned wormhole connections and sov upgrade counts ride in the live
payload. Sov arrives as a **resolved colour** rather than an alliance id, so the page agrees with the
app without knowing that alliances exist. Each layer toggles, and the choice is per device.

Routing is deliberately absent, as asked.

## The smaller ones

- **Alerts** drop the severity and age header. The card already carries both.
- **Pings** drop the "matched … rule" line. `render_ping` shows the highlight and nothing else; the
  line was invented, and it is gone.
- **Card padding** was `8px 4px`. `intel_row` uses `Margin::symmetric(8, 4)`, which is 8 *horizontal*
  and 4 vertical, and CSS shorthand is the other way round. Now `4px 8px`.
- **Distances** read as AU at 0.1 AU and above, one decimal. The app shows km throughout; on a phone
  a seven-digit km figure is noise.
- **zKill cards** no longer carry the trailing `KILL` tag, which repeated what the card already says
  with its icon and its own dark background.
- **A disabled pane leaves the header bar** rather than sitting there dimmed. It comes back from the
  layout menu, which is where it was switched off.
- **Scroll survives a re-render**, which matters much less now that re-renders are rare, but "rare"
  is not "never".

## Join comms

The Join button posts `JoinComms { ts }` and the desktop opens the link. The phone cannot use a
`mumble://` link and the client is on the desktop anyway.

It names a **ping**, not a URL. This socket is reachable from the network, and a message carrying a
URL would be a way to make the machine open anything; naming a ping lets the app look up a link it
already holds. `join_comms_names_a_ping_and_never_a_url` asserts the protocol has no shape that
accepts one.

## Verified

`after/web-1400.png`: the canvas map with its layer toggles, the camp ring, the sov wash and the
position ring, beside an alerts pane with no duplicated header. The publish-rate measurement is in
the reply that accompanied this work.
