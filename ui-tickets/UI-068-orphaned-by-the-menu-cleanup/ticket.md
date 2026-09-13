# UI-068 &mdash; Three features lost their only entry point when the map menu was cleaned

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `app.rs` wormholes view, route panel, map mode picker |
| **Found by** | a dead-code sweep after the user asked what was still outstanding |

## Symptom

The compiler reported five functions as never used. Four of them turned out to be features with no
remaining way to reach them, not merely tidy-up:

| | |
|---|---|
| `kill_wormhole` | "Mark hole dead" was only in the map's context menu. No other UI marks a hole dead, and `UPDATE wormholes SET dead = 1` has one writer. |
| `toggle_dock_permit` | Capitals/supers dock permits were only in that menu. Existing permits still drew their teal ring; nothing could set or clear one. |
| `recompute_jump_route` | Its only caller went when the route panel stopped handing jump routes to the old planner. `jump_route`, `jump_legs` and `jump_alt` are still *read* by the `JumpPlan` map mode, which is still selectable and now draws nothing. |
| `char_missing_scope` | The helper is orphaned, but the warning it fed is computed inline in `characters_view`, so the feature survives. |

## Cause

UI-053 cut the map's context menu down to the web's entries plus Favourite, as asked. Three of the
entries removed were the only door to their feature, and the dead code they left was the only sign.

This is the second time in this round that removing a switch left what it switched stranded: UI-066
was the same shape, with the fleet poller waiting on a flag nothing set.

## How to verify

Mark a hole dead, set a dock permit, and select every map mode. A fix would be WRONG if it only
deleted the dead functions: that makes the compiler quiet and the features stay gone.
