# GAP-002 &mdash; Five of nine views screenshot a placeholder

| | |
|---|---|
| **Type** | Harness coverage gap |
| **Status** | Not scheduled |
| **Blocks** | Map, Intel, Battles, Characters, Dashboard |
| **Effort** | Medium |

## What the harness cannot reach

The scratch DB is created but never seeded, and `root_chrome`/`root_central` skip the prologue that would populate state. Map shows the SDE download prompt, Intel early-returns a red error line because `chat_dir` is `None`, Battles sits on a spinner.

## Route in

`sde_ready()` (store.rs:411) needs one `sde_systems` row and a matching `sde_meta.schema`, so the Map unlock is cheap. Add a test-only seeding seam on `Store`, plus a way to set `chat_dir` and `systems` without running the prologue.

## Note

Coverage gaps are tool work, not app defects. They are ticketed here so the backlog is complete,
but the UI-NNN defects run first: closing those needs no new harness capability.

## Concrete consumers

- **UI-004** fixed two call sites of the kill-intel range control. Only the alerts one is covered by
  a permanent scene. The intel toolbar site needs `chat_dir` set, or `intel_view` early-returns on
  its "EVE chat logs not found" branch and the toolbar never draws, so half that fix is verified
  only by throwaway scenes that no longer exist.

## Related checker blind spot

`checks.rs` flags horizontal escape but not vertical, because scrolled content below the fold is
normal and vertical escape is ambiguous. UI-021 hit the consequence: a composer running off the
bottom of a small window would not have failed anything. Worth revisiting alongside seeded scenes,
where a panel's true content height becomes knowable.
