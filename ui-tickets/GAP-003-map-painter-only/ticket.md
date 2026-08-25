# GAP-003 &mdash; Map and threat view are ~1,900 lines of painter-only canvas

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Not scheduled |
| **Blocks** | ~80% of the Map view's pixels |
| **Effort** | Medium |

## What the harness cannot reach

One `allocate_rect` then ~80 raw `painter.*` calls (app.rs:9291, app.rs:10430). Every system dot, gate line, route path and range ring emits no AccessKit node, so kittest cannot query them and `checks.rs` cannot see their rects.

## Route in

Add `widget_info` to the map rect, then drive it by coordinate with `harness::click_at` computed from `map::project`. Gated behind GAP-002: there is nothing to draw until the SDE is seeded.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.
