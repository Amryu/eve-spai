# WEB-007 &mdash; Alerts and fleet pings do not exist in the browser

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `assets/panes-alerts.{js,css}`, `assets/panes-pings.{js,css}` |
| **Reported by** | user, remote web view |

## Gap

The reason to look at a phone is that a rule fired or an FC pinged. Neither renders.

## Deliverable

**Alerts pane.** Rows carrying the severity colour from `severity_color` (`app.rs:22920`), the rule
that fired, the fire time, and the same card body as the intel pane. Rows not yet seen on this device
are marked, and the marker is per device.

**Pings pane.** Cards reproducing `render_ping` (`app.rs:22700`): the megaphone header with fleet name
and PAP tag (`STRAT` red, `PEACE` amber, free text weak), FC, formup, comms with the Mumble join link,
doctrine link, body, and the weak `source -> target` footer. The accent stroke and 8% accent fill when
a `PingRule` matched, matching `matching_ping_rule` (`app.rs:1934`).

## Notes

`pings::Ping` has two variants and `Plain` is not a degenerate `Fleet`; render it as its own shape.

Rescue is out of scope. `rescue::RescueState` has no serde derives and the whole module is behind the
off-by-default `fc-rescue` feature. Say so here so the next round does not relitigate it.

UI-003 found the ping body rendering dimmer than a routine reminder, and UI-018 found its lines
allocating 26px for 15px of ink. Both were fixed in the app; do not reintroduce either in CSS.

## How to verify

- A Rust test asserting the ping DTO covers every field of both `Ping` variants, including
  `Formup::System` resolving to a name.
- WEB-005 demo screenshots of `ping_fleet`, `ping_fleet_no_doctrine`, `ping_plain` and
  `ping_plain_multiline`, at phone and desktop width.
- A fix would be WRONG if it dropped the matched-rule highlight, which is the only thing separating a
  ping that concerns you from one that does not.
