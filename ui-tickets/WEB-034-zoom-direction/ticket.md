# WEB-034 &mdash; The wheel zooms the wrong way

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `assets/map.js` |
| **Reported by** | user |

## Symptom

"Zoom direction is wrong, otherwise feels okay"

## Cause

WEB-033 carried the app's `exp(scroll * 0.003)` across with the sign it has there. The app's
`map_zoom` is a magnification, which goes up as you zoom in; the web map's `k` is map units per
pixel, which goes down. Negating the exponent to account for the browser's sign convention then
negated it a second time.
