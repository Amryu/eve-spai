# GAP-006 &mdash; The alert auto-dismiss path is never exercised

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Closed, see `review.md` |
| **Blocks** | the behaviour that decides whether the overlay blocks game clicks |
| **Effort** | Small |

## What the harness cannot reach

The harness fixtures set `pinned: true`, which makes `active` permanently true at app.rs:19832. The transition to hidden plus `MousePassthrough` is never reached.

## Route in

Add a scene with `pinned: false` and enough steps to drain the countdown.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.
