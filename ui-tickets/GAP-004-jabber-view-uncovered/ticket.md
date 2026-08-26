# GAP-004 &mdash; The Jabber view is the tenth of ten and covered zero times

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Popout surface covered, see `review.md`. In-app view still uncovered. |
| **Blocks** | ~1,200 lines |
| **Effort** | Small |

## What the harness cannot reach

`root_central` renders it only when handed a `JabberFrame`, and the harness passes `None`. All 15 fields of that struct are private, and the builder probes the OS keyring, which CI does not have.

## Route in

The underlying `JabberState` is fully public with a `Default`, so only the struct wall and the keyring probe are in the way. One visibility change plus a `has_password` override unlocks the view, the popouts and the tab bar.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.
