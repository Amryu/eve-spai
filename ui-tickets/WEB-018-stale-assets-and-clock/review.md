# WEB-018 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-018-churn-and-menu`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 673 to 675 |
| **Follow-ups** | map parity, taken separately at the user's request |

## The one that was wasting everyone's time

The user reported a bug I had already fixed, in a build that was already running. Checking the
server rather than arguing about it took one command: the installed binary and the live server both
served the new markup, and the browser rendered the old.

Assets had an `ETag` and no `Cache-Control`. An `ETag` answers "has this changed" but only when the
browser asks, and with no freshness information at all a browser is entitled to decide for itself how
long to go without asking. `no-cache` makes it ask every time, which costs one 304 for an unchanged
file. It does not mean "do not store", and the test asserts the header is present rather than absent
so nobody later "optimises" it into `no-store`.

Worth noting the shape of the mistake: WEB-013 fixed a stale-asset bug by making the `ETag` correct,
and stopped there. A correct validator with no freshness policy is half a cache.

## The churn, one layer deeper

`UncertainPilots` wraps a `HashSet`, is rebuilt every tick by `uncertain_set`, and is serialized into
the snapshot that gets hashed. Exactly the WEB-017 fault, inside a type whose container was not
visible from the snapshot definition. It is a `BTreeSet` now.

`a_snapshot_hashes_the_same_when_rebuilt` hashes a rebuilt `Lookups` thirty times over. It was
checked by reverting the set to `HashSet`, and it fails.

That is the third container to cause this, so the test is written against `Lookups` as a whole rather
than against any one field: anything unordered added to the snapshot from here on fails it.

## The clock

Ages are rendered at paint time, and WEB-017 correctly stopped repainting panes. The age spans carry
`data-at` and a one-second interval rewrites their text in place, touching nothing else. Rebuilding a
pane every second to move a clock is what this page has spent three tickets escaping.

## The swipe

`apply()` runs after every render and called `scrollToActive` unconditionally, snapping the strip
mid-animation. It now skips while the strip is settling, and `scrollToActive` returns early when it
is already where it wants to be, so a finished animation is not restarted.

## Map dots

Radius clamped to 3px from 5.5, with the overlay rings as multiples so they shrink with it. Full
read-only parity with the app's map is a separate piece of work, deferred at the user's request
because it is the most complex part.
