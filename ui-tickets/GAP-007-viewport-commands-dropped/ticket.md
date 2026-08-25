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
