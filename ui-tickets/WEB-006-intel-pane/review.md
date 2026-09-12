# WEB-006 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-006-intel-pane`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 638 to 639 |
| **Follow-ups** | none |

## The colours came out of `app.rs` first

`intel_row` carried fourteen colour literals inline: `0x8e,0xd6,0xe6` for a celestial, `0x4a,0x3d,0x10`
behind an ISK chip, and so on. Reproducing the card meant copying those hex values into CSS and hoping
neither side was ever edited alone.

They are `theme::chip` constants now, named for what they are, and `intel_row` reads them. `web::css`
emits the same constants as custom properties, so the page names no colour at all. Extracting fourteen
values by hand is exactly where a digit slips, so
`chip_colours_match_the_literals_they_replaced` pins every one to the literal it came from.

## What the card reproduces

The chip order from `intel_row`: type icon, monospace age, character ring or plain jump number,
system badge, near-celestial, hostile count, ISK, structures, celestials, probes, hull badges,
ambiguous hull, ship classes, tackled, pilot badges, gates, link chips, flag tags, movement hint, and
the `reporter · channel` footer. Severity tint at 13%, 5% when stale. Compact is density only, as in
the app: row 28 to 16, hull icons 24 to 16, portraits 20 to 16.

Two behaviours were copied deliberately because getting them wrong would make the two feeds disagree
about what happened:

- **A pilot with no resolved id is skipped**, as `intel_row` does. Rendering the raw name would show
  the phone names the desktop does not.
- **The character ring is empty for a single character.** `CardChars` only fills in when there is
  more than one character to confuse; with one, the card draws the plain number.

## Verified

`after/web-1440.png` and `after/web-390.png`, from the fixture demo.

The 1440 shot shows `intel_typical`: the warning icon tinted by severity, `46s`, `here`, `1DQ1-A` in
the magenta the security ramp gives -0.36, a hostile count of 3 on red, Muninn and Loki with their
CDN hull icons, both pilots with portraits, `Second Target` carrying the amber `?` that marks an
uncertain name, and the reporter footer. That is the same card the app draws, which is the point.
