# WEB-036 &mdash; Ship window is a fraction of the app's, pilot chips have no affiliation, jabber reads badly

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/detail.rs`, `assets/dialogs.*`, `assets/panes-intel.*`, `assets/panes-jabber.*` |
| **Reported by** | user |

## Symptoms

1. "Ship data window: Add colors for damage types, bonuses are missing the values and associated
   skill. Make sure this window shows the same info as the app one."
2. "The player badges are missing alliance/corporation icons"
3. "Web jabber: the '@goonfleet.com' suffix is not needed, shows seconds precision for message times.
   Don't make the names of the other participant gray, it's hard to see. Just make it another shade
   of the 'me' color"

## Cause

1. `ShipInfo` carried a fraction of `ShipDetails` and threw away two thirds of a trait: `ship_traits`
   returns `(skill_id, bonus, text)` and the DTO mapped it to `text` alone. No slots, no velocity, no
   warp speed, no EHP, no role badges, and four bare percentages with nothing to say which damage
   type each one was.

2. The affiliations are already in the snapshot, keyed by character id, and the chip drew the
   portrait only.

3. Senders arrive as bare JIDs in a DM, so every line carried the domain. The other participant's
   name was `--muted`, which made the half of the conversation you did not write the harder half to
   read.
