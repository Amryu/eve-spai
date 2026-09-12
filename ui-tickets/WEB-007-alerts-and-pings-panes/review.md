# WEB-007 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-007-alerts-pings`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 639 to 640 |
| **Follow-ups** | none |

## What changed

**Alerts** render the intel card as their body, with a severity header above it. Sharing the card is
the point: an alert and its feed entry are the same event, and drawing them differently would invite
someone to read them as two.

Rows this device has not seen carry an accent edge, tracked in `localStorage` and bounded at 300 ids,
since the feed itself is capped at 100. Per device deliberately: two people watching one app should
not clear each other's markers. A browser that refuses storage reads everything as unseen, which is
the safe way round.

**Pings** reproduce `render_ping`: the megaphone header with fleet name and PAP tag (`STRAT` red,
`PEACE` amber, free text weak), FC, formup, comms with the Mumble join link, doctrine, body, and the
`source → target` footer. A matched rule gets the 2px accent border and the 8% accent wash. `Plain`
is rendered as its own shape rather than as a fleet ping with empty fields.

Line heights are set against the ink. UI-018 and UI-027 both landed on ping and chat bodies
allocating 26px of row for 15px of text; there was no reason to reintroduce it here.

## A defect the screenshot caught

The first render showed `Formup: 30004759`. `Formup::System` carries an id and nothing else, and the
page has no SDE to look it up in, so a formup pointed the reader at a number.

`PingPane` now carries a `systems` map with names for exactly the ids the formups reference, resolved
by the publisher from `geo::Systems`. `a_formup_system_is_named_for_the_page` covers it. Worth noting
how it was found: every Rust test passed both before and after, because nothing in the suite knew what
a formup is supposed to read like.

## Verified

`after/web-1440.png` and `after/web-390.png`. The phone shot is the useful one: the cards wrap rather
than overflow, the formup reads `1DQ1-A`, and the ping's Mumble link and doctrine survive the narrow
width.
