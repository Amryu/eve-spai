# WEB-036 review cycle

**Status:** Delivered
**Branch:** `web/web-036-detail-parity`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 693, unchanged |
| **Follow-ups** | none |

## The ship window

Everything the app's shows: the resist table with a bar per damage type in the app's own colours,
per-layer EHP and a total from `layer_ehp`, which is now shared rather than reimplemented so the two
windows cannot disagree about a number read off both. Hardpoints, slots, drones, velocity, warp
speed. Role badges as the app's own Phosphor glyphs, sent as glyphs rather than names because the page
already serves that font and a second mapping is a second thing to keep in step.

Bonuses are grouped by skill with their values, and role bonuses under their own heading, which is
what the app does. Skill names come from ESI and the app caches them; a hull the app has never opened
has none, so the route resolves the misses itself. One blocking call per hull that has never been
looked at anywhere, then never again, which beats showing "Skill 3330".

## Pilot chips

Alliance, corp, portrait, name, in the app's order. The affiliations were already in the snapshot and
the chip was drawing one of the three images it had the data for. The demo fixture carries no
affiliations, so `after/` shows the chips without them; the live app has them.

## Jabber

Senders lose the domain. A DM's sender arrives as a bare JID, so every line in a conversation with
one person repeated the same domain. The full timestamp moves to the title, where seconds are
available without being in the way.

Both halves of a conversation are the accent now, at two strengths. `--muted` for the other person
made the half you did not write the harder half to read, which is backwards.
