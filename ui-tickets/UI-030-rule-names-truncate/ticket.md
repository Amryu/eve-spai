# UI-030 &mdash; Alert rule names truncate heavily in the default panel

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | the alert rule list in `alert_rules_editor` |
| **Found by** | UI-019 |

## Symptom

In the 240px default rule panel, names truncate to about eight characters: "Hostiles near home"
renders as "Hostile…", "Quiet hours" as "Quiet h…". Three rules in the scene, all truncated.

## Why it got worse, and why that was still right

UI-019 replaced a hardcoded `54.0` reserve for the two reorder arrows with their measured width
(82px). The old number was too small, so a name only fitted by running its click rect **under the
arrows**, which `uitest_layout` caught as overlapping click targets. Reserving the true width fixes
the overlap and costs name width.

So this is not a regression to revert. It is a pre-existing narrow-panel problem that the overlap was
previously hiding.

## Options

- Widen the default panel. Simplest, costs horizontal room in the editor.
- Put the arrows on a second row, or reveal them on hover, so the name gets the full width.
- Truncate in the middle rather than the end, so "Hostiles near home" keeps its tail.
- A tooltip carrying the full name. Weakest on its own: the list exists to be scanned.

## Note

The pre-existing `.max(40.0)` floor means a user dragging the panel to its 180px minimum can still
overlap. The harness cannot reach that width today.

## How to verify

`view_alert_rules` covers this at the default width. Add a narrow variant, and check
`uitest_layout` stays green at both.
