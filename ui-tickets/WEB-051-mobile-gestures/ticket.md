# WEB-051 &mdash; A route drag eats the pinch and every pan that starts on a system

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `assets/map.js` |
| **Reported by** | user |

## Symptoms

"The mobile map zoom isn't working well. Also the dragging from a system gets in the way of moving and
panning quite a bit there. On mobile, dragging routes should not work until a route has been started."

## Cause

Both are the same gesture taking priority over the others.

A route drag was handled first in `pointermove` and returned, so once a finger had landed on a system
the second finger's pinch never ran at all: zoom simply stopped working anywhere near a system, which
on a star map is most of it.

And a drag off a system is a route on a desktop because there is always somewhere else to press. On a
phone with the map filling the pane there often is not, so panning turned into drawing a line.

## How to verify

Pinch with one finger starting on a system. A fix that only reorders the branches would be WRONG: the
first finger has to stop what it was doing, not queue behind it.
