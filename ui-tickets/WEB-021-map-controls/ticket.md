# WEB-021 &mdash; Map controls: Fit, the layer panel, upgrade marks, jump range, and the system window

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/{map,dialogs}.{js,css}`, `web/map.rs`, `ipc.rs` |
| **Reported by** | user |

## Symptom

- "Clicking a system on the map should not open a dialog, but a floating, dismissable window and the
  system should be selected in the app."
- "The jump range overlay is missing here."
- "'Upgrades' filter doesnt seem to be working or the icons are incredibly tiny."
- "The 'Layers' should be open all the time on desktop unless the viewport gets too small. Clicking
  elsewhere should only close it on mobile."
- "Remove the 'Fit' button from the web map."

## Cause

**Upgrade marks** were sized from the dot radius, which is clamped at 3px, so a mark was under three
pixels across at every zoom. The filter worked; nothing it drew was visible.

**Jump range** needs real 3D positions. The geometry carried only the drawn coordinates, which are a
projection quantised into a 0..4096 box and cannot answer "is this within 6 light years".

**The layer panel** was an overlay anchored to its button at every size, and a click anywhere outside
closed it, including a click on the map it describes.

**The system dialog** was a modal: it took the whole page, dimmed behind it, and blocked the map that
was the reason it was open.

## How to verify

On a wide pane the layers are visible without a click and stay visible while the map is used. Upgrade
marks are legible. Hovering a system with jump range on draws the bands and tints what is in them.
Clicking a system floats a window and moves the desktop map to that system. A fix would be WRONG if
jump range were computed from the drawn coordinates, which would make it wrong by however much the
projection distorts.
