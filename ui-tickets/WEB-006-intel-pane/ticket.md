# WEB-006 &mdash; The intel feed does not exist in the browser

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `assets/panes-intel.{js,css}` |
| **Reported by** | user, remote web view |

## Gap

The snapshot carries the feed and the page can receive it, but nothing renders a card.

## Deliverable

A card reproducing `intel_row` (`app.rs:23270`), in its chip order: type icon, monospace age,
character jump slot, system badge, near-celestial, hostile count, ISK, structures, probes, ship
badges, ambiguous ship, ship classes, tackled, pilot badges, gates, alliance logos, link chips, flag
tags, movement hint. Severity tint at 13% alpha, 5% when stale. `security_color` on system badges.
Amber fill and a trailing `?` on uncertain pilots. Portraits, corp and alliance logos and hull icons
from the EVE image CDN at the same bucket sizes `eve_img_size` picks (`app.rs:20578`).

Filters mirroring the app: `IntelTypeFilter` (`app.rs:13`, All / Hostile / Clear / Kill / Threat with
`matches` at `app.rs:347`), the text query over text, channel and system name, and `<= N jumps`.
Applied client-side over the snapshot, same 250-card cap.

Compact mode as pure density, matching what the app actually changes: row height 28 to 16, hull icons
24 to 16, portraits 20 to 16, tighter gaps, `fmt_age_compact` formatting. Nothing is hidden.

## Notes

The card carries `received` as a unix timestamp, not a formatted age, so the clock ticks client-side
and the compact toggle needs no round trip.

`intel_row` skips any pilot missing from `resolved_pilots`. The web card must do the same or it will
render names the app does not.

`uncertain` is `pilot::UncertainPilots`, which lowercases on construction and matches
case-insensitively. Do not re-implement that matching with a plain set.

No small font sizes on content text. That rule cost this repo a sixteen-site audit (UI-008).

## How to verify

- WEB-005 demo, screenshots at 390px and 1440px, compact and normal, placed beside the egui
  `uitest` PNG of the same fixture so the two can be compared directly.
- A Rust test that the intel pane DTO carries every field `intel_row` reads, so a chip cannot silently
  render empty.
- A fix would be WRONG if it hardcoded a colour rather than reading the CSS variables WEB-003 emits.
