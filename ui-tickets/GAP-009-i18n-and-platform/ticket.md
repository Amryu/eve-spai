# GAP-009 &mdash; CJK layout, platform branches and fc-rescue are uncovered

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Not scheduled |
| **Blocks** | i18n overflow, 2 rescue windows |
| **Effort** | Small, large for cross-platform |

## What the harness cannot reach

The harness skips the system CJK font probe for determinism, so i18n layout is never rendered, and the app has an `sde_ship_i18n` table meaning CJK ship names are real inputs that can overflow every chip. Windows and macOS `cfg!` branches compile but take the opposite path on Linux.

## Route in

Add a CJK fixture with a bundled font. Add rescue scenes under `--features fc-rescue`. Real cross-platform layout needs Windows and macOS runners.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.
