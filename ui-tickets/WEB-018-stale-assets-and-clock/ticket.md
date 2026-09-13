# WEB-018 &mdash; Fixed pages kept rendering the old bug, the clock stopped, and the swipe cut off

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `web/server.rs`, `pilot.rs`, `assets/{panes-intel,layout,map}.js` |
| **Reported by** | user, running the installed build |

## Symptom

- "The alert type and time is still showing above each alert card" &mdash; after the build that
  removed it was installed and running.
- "The time since the alert is not updating each second as it should."
- "The left/right sliding animation is not smooth. It kinda cuts off at the end."
- Map dots still too big.

## Measured

The server was checked directly while the user was seeing the old behaviour:

| | |
|---|---|
| `ahead` in the installed binary | 0 |
| `ahead` in what the running server serves | 0 |
| What the browser rendered | the old markup |

So the fix shipped and the browser did not take it.

Publish rate, continuing from WEB-017:

| | |
|---|---|
| Publishes in 12 idle seconds, before WEB-017 | 70 |
| After the `BTreeMap` fix | 22 |
| Expected with four panes idle | ~0 |

## Cause

**Stale assets.** Assets were served with an `ETag` and **no `Cache-Control`**. That is no freshness
information at all, so a browser applies its own heuristic and may serve a stored copy without
revalidating. The content-keyed `ETag` from WEB-013 only helps once the browser decides to ask.

**The clock.** Ages are rendered at paint time. WEB-017 made panes rebuild rarely, which is correct,
and took the clock with it.

**The swipe.** `apply()` runs on every render and called `scrollToActive` unconditionally, which
snapped the strip mid-animation.

**The remaining churn.** `UncertainPilots` wraps a `HashSet`, is rebuilt every tick, and is
serialized into the snapshot. Same fault as the lookup maps in WEB-017, one layer deeper.

## How to verify

Change an asset, reload once, see the change. Watch an age tick without the pane rebuilding. Swipe
between panes and watch it settle rather than snap. A fix would be WRONG if it stopped the browser
caching altogether: revalidation is one 304, re-downloading everything on every load is not.
