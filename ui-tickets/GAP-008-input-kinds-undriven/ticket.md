# GAP-008 &mdash; Drag, right-click, wheel and file drop are undriven

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Not scheduled |
| **Blocks** | 4 context menus, 2 DnD systems, map zoom |
| **Effort** | Small, except tab drag |

## What the harness cannot reach

kittest already offers `click_secondary`, `drag_at`/`drop_at` and raw events; none are used. Jabber tab drag-and-drop is the hard one: multi-pass press-move-release on a painter-only rect with no queryable node, reading a viewport rect kittest never populates.

## Route in

Parameterise `click_at` on button, add a `wheel_at` helper, set `input_mut().dropped_files` for the file-drop path.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.
