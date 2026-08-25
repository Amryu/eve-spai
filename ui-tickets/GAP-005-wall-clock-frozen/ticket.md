# GAP-005 &mdash; Instant-gated behaviour cannot be tested

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Not scheduled |
| **Blocks** | 13 sites, incl. the map tooltip |
| **Effort** | Small-medium |

## What the harness cannot reach

`ctx.input(i.time)` advances at 0.25s per step so egui's own tooltip delay works, but `Instant::now()` does not. The map hover tooltip needs 500ms of real elapsed time and can never open in a test. Double-click is also unreachable: one event per pass at 0.25s exceeds the 0.3s window.

## Route in

`with_step_dt` and more steps fix double-click. `Instant`-gated code needs a `now_instant()` seam the harness can advance.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.
