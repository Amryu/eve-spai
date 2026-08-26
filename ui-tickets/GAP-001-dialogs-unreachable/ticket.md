# GAP-001 &mdash; 28 dialogs and secondary windows are unreachable

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Mechanism proved, 8 of 28 covered, see `review.md` |
| **Blocks** | ~6,000 lines |
| **Effort** | Medium |

## What the harness cannot reach

The harness calls `root_chrome` + `root_central`, so the dialog block of `App::ui` never runs. Every dialog is gated on a private `SpaiApp` field, and one of ~332 fields is visible outside `crate::app`.

## Route in

Extract a `root_dialogs(&mut self, ctx)` alongside the existing two, make the gate fields `pub(crate)`, add one `Scene::ctx` per dialog. Prefer this over `egui_kittest`'s `build_eframe`, which would drag the whole side-effecting prologue back in.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.
