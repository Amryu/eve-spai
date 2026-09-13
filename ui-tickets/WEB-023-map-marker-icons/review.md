# WEB-023 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-023-map-icons`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 689 to 690 |
| **Follow-ups** | none |

## What changed

Markers draw `state.icons` glyphs in the Phosphor font the page already serves, which is the same
file the app draws with, so the same marker is the same picture in both.

Four names were missing from `web::icons::ICONS` and are added: radioactive, gear, cell tower and
crosshair-simple. They are emitted from `egui_phosphor::regular` like the rest, so nothing here
hardcodes a codepoint.

A mining upgrade draws the ore's own EVE type icon, fetched once per type and cached, with a repaint
when it arrives.

## The part that would have put the boxes back

Canvas falls back **silently** when asked for a webfont it has not loaded, and the fallback for a
private-use codepoint is a box. So the first paint of every load would have drawn exactly the squares
being fixed.

`document.fonts.load` gates it, with a repaint when the font arrives, and a filled dot stands in
until then rather than a box. `the_map_waits_for_the_icon_font` asserts both halves.

## Gate camps keep their ring

A campfire glyph alone disappears against a dense field of systems. The red ring is what carries at a
glance and the glyph says which kind of marker it is, so both are drawn.

## Verified

`after/icons.png`: the campfire on the camped system, the spiral on the wormhole end, and the route
running between them.
