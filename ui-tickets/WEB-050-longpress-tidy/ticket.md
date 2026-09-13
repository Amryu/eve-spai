# WEB-050 &mdash; The long press opens the menu and the tap underneath it

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | `assets/map.js`, `assets/route.js` |
| **Reported by** | user |

## Question

"Does the context menu works when holding down on a system for a second (or whatever the usual delay
is for this on mobile)"

Yes, 500ms, since WEB-044. Two things wrong with it, found while checking.

## Cause

1. A long press ends with a finger coming off the glass and the browser sends a click for that, so the
   menu opened and the tap under it opened the system window as well.

2. `menu()` removed the previous menu's element but left its outside-click listener registered, so
   opening a menu a few times left a few of them behind.
