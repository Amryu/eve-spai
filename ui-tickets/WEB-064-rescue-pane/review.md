# WEB-064 review cycle

**Status:** Delivered
**Branch:** `feat/web-064-rescue-pane`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Suite** | 705 without the feature, 744 with it |
| **Follow-ups** | none |

## The pane

Read-only, deliberately. The desktop's rescue window sends pings and pulls people into comms; a page
reachable from the LAN should be able to watch that and not do it. So the pane carries the call list,
which one the FC is working, the capital and its pilot, and the range block that is the whole question
the mode exists to answer: how far the staging is, the closest system in titan range, and what it
costs by ansiblex and by gate.

The types are unconditional and only the filling is `cfg`'d. A build without `fc-rescue` never
publishes the pane, which keeps `cfg` out of the snapshot and out of the page, and the page learns
whether it exists from one flag in `meta`.

## Always on where it exists

Rescue is already an explicit choice made twice, in the build and in the settings. A third switch to
forget is one too many, so where the pane exists it is on and it is not in the visibility menu.

## The budget

Everything switched on stays switched on. Tabs shows all of them; grid and columns take the ones that
fit, in order, and leave the rest alone. Switching a pane on used to switch another off, which is a
layout quietly discarding a decision.

`after/five-enabled-four-drawn.png`: five panes enabled, five tabs, four columns drawn, and the fifth
still enabled rather than turned off behind the user's back.
