# WEB-054 &mdash; The avoid list cannot be inspected, and three controls are too small to hit

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/dialogs.*`, `assets/map.css` |
| **Reported by** | user |

## Symptoms

1. "'Plan around this system' is poor wording, call it avoiding. Also, on mobile that avoidance button
   is really small and you can't hover it. Make sure the avoidance list can also be inspected (inform
   the user how many systems are being avoided in this jump planning window/sidebar, both app and web)."
2. "The size of this docked window next to the map should not change because tabs are being switched."
3. "The 'X' to close the window is also way too small for mobile"

## Cause

1. The avoid button appeared on hover, which a touch screen does not have, and the count line said how
   many without saying which.
2. The dock was content-sized with a cap, so every tab gave it a different height and the map moved.
3. Three glyph-sized targets: the window close, the tab close, and the avoid button.
