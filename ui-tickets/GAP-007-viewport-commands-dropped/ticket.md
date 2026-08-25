# GAP-007 &mdash; Viewport commands go nowhere, and the overlay is a separate process

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Not scheduled |
| **Blocks** | 12 command sites, a 479-line second binary |
| **Effort** | Small to assert, large to verify |

## What the harness cannot reach

kittest reads only the root viewport, so every `send_viewport_cmd` is dropped. Worse, in production the overlay runs as a separate process over stdin/stdout IPC and `alert_window` returns early when it exists, so the harness exercises the in-process fallback rather than the shipping path.

## Route in

Assert the command sequence by reading `harness.output()` after a step. Verifying real window behaviour needs something kittest does not provide.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.

## Partly closed

UI-007 added `uitest_alert_titlebar_has_no_competing_grab_target`, which reads
`harness.output().viewport_output` for `ViewportCommand::StartDrag` and asserts it fires from four
points along the alert title bar. The technique works, so asserting the command sequence is now a
solved problem and the remaining 11 `send_viewport_cmd` sites can follow the same pattern.

Still open: real multi-window behaviour, and the overlay subprocess.

## The harness cannot see the OS frame

It renders viewport contents only, so a decorated window screenshots identically to an undecorated
one. UI-016 was raised as a defect on exactly this basis and closed as a false positive: the ping
window keeps the window manager's title bar and close button, which never appear in a render.

Any ticket about window furniture needs `with_decorations` checked in source before it is believed.
