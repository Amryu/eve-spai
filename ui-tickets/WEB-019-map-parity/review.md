# WEB-019 review cycle

**Status:** Fixed and verified, with three gaps recorded
**Branch:** `web/web-019-map-parity`

## Resolution

| | |
|---|---|
| **Outcome** | Parity for the read-only layers, minus three recorded below |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 675 to 677 |
| **Follow-ups** | none |

## Parity came out of fixing the payload, not out of adding to it

Everything the map was missing was **already being sent**. `SysFlags` carries ADM, all four activity
counters, sovereignty and incursion, and it was in the snapshot twice over: once in the intel pane's
lookups and once inside the embedded alert message. The map could not read either, because they
belonged to other panes.

So `status` is its own top-level pane now, shared, with its own revision. That is also the right
cadence: ESI updates it every few minutes while intel moves constantly, and inside the intel pane it
was re-sent on every report.

Measured against the real snapshot:

| | Before | After |
|---|---|---|
| `status`, encoded | 1,043,628 | 378,549 |
| `status`, copies sent | 2 | 1 |
| pings | 737,593 (1163) | 52,624 (80) |
| **snapshot** | **3,153,157** | **759,481** |

The encoding change is `SysInfo`: short names, everything optional skipped, and sovereignty resolved
to a colour server-side so the page never has to know an alliance exists.

## The layers

Sovereignty by alliance **and** by coalition, activity on the app's own yellow-to-red ramp for all
four counters, ADM as a number beside the system, cyno generators, sov upgrades marked by kind and
level, bridges, wormholes, camps, Jove and labels.

Upgrades are classified in Rust by the app's own `upgrade_info` and `upgrade_kind`, so the same
upgrade gets the same reading on both. Copying the keyword list into JavaScript would have been two
lists to keep in step.

Double-click frames a region, which is the app's region view; Fit returns to the universe.

## Controls, on both

Ten controls in a row is most of a phone's screen and a third of a pane on a desktop, and the map is
what deserves the space. They sit behind one Layers button at every width: anchored under it on a
desktop, a sheet on a phone. Same markup, because maintaining two is how they drift. `#maplayers`
opens it on load, since the harness cannot click.

## Not at parity, deliberately

Three things are absent and are not oversights:

- **Jump range on hover.** Interactive and routing-adjacent, which the user excluded.
- **The Radial and Tree threat layouts.** These are alternative graph layouts, not overlays; porting
  them is a separate piece of work with its own geometry.
- **Thera and Turnur hub toggles.** Scanned holes are drawn, but the two hubs do not have their own
  switches.

## Verified

`after/lay-desktop.png` and `after/lay-phone.png`: the layer panel with Sov and Activity as cycles
and the eight toggles, over a map drawing the sov wash, the camp ring and the position ring.
