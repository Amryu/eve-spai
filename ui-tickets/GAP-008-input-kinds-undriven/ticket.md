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

## Correction from UI-023

This ticket called jabber tab drag-and-drop large effort and effectively untestable. Too pessimistic.

`harness.event` with coordinates **does** drive the tab drag: press at the tab's centre, move 8px to
cross the threshold, move to the target, release. UI-023 asserts both the appearance and the
disappearance of the drag ghost that way.

What is genuinely unavailable: `get_by_label().click()` on painter-only tabs, since they emit no
AccessKit node, and cross-window drops, which read a viewport rect kittest never fills.

The workaround for painter-only surfaces is to assert on `harness.output().shapes`, recursing
`Shape::Vec` and matching `Shape::Text` galleys. That generalises to GAP-003's map canvas.
