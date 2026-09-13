# WEB-044 &mdash; The web map has no context menu

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js`, `assets/route.js`, `assets/map.css` |
| **Reported by** | user |

## Ask

"Web app: Add the context menu: 'Start Jump Route', 'Start Gate Route', 'Start Titan Route' and
afterwards 'Add Waypoint', 'Remove Waypoint', etc. options"

## Gap

The drag gesture could only build a route forwards, one leg at a time, and there was no way to insert
a waypoint between two systems already on it or to take one out. The desktop map has had a context
menu since long before any of this; the web one had nothing on right-click but the browser's own.

## Acceptance

Reachable on a phone as well, where there is no right button.
