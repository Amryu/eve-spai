# UI-065 &mdash; Rescue mode is a second switch, a second window, and a map that changes by itself

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `app.rs`, `nav.rs` |
| **Reported by** | user |

## Asks

1. "'Rescue mode' is always active while the feature is enabled. No more switching inbetween. It also
   no longer should affect what the map displays (so it might as well be removed as a concept)"
2. "The rescue window should also no longer be a separate window. Instead it becomes part of the main
   window and will get it's own tab on the left, below the jabber one."

## Cause

Rescue was three things at once: a build feature, a settings checkbox, and a runtime mode you entered
and left. Three states for one decision, and each of the window, the map and the ping handling had its
own idea of whether a rescue was happening.

The map's part of that was the worst of it: entering the mode replaced the overlay set with a preset,
so the map changed under whoever was reading it, for a reason they did not ask for and could not see,
and their own layer switches were ignored until they left.
