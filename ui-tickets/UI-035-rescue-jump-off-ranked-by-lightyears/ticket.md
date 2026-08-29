# UI-035 &mdash; The rescue window picks a jump-off system by map distance, not by jumps

| | |
|---|---|
| **Severity** | High |
| **Status** | Fixed, see `review.md` |
| **Region** | `update_rescue_range` in `app/src/app.rs`, behind `fc-rescue` |
| **Reported by** | user |

## Symptom

With the stranded capital in **5J4K-9**, the out-of-titan-range banner named **ZH-GKG** as the
closest reachable jump-off point. ZH-GKG is a long way out by gates. Two systems one jump from the
target were in titan range of staging the whole time and were never offered.

## Cause

`app/src/app.rs`, `update_rescue_range`. Candidates were filtered to everything a titan can reach
from staging, then ranked with:

```rust
.min_by(|a, b| {
    crate::map::ly_distance(a, target_pos)
        .total_cmp(&crate::map::ly_distance(b, target_pos))
})
```

That is straight-line distance on the map. Null-sec gate topology does not follow the map, so the
system that looks nearest to the target is regularly many gates from it, while a direct neighbour of
the target sits slightly further out in lightyears and loses.

The jump counts were computed **after** the winner was chosen, purely for display, so the banner
reported the true route length of a system picked for the wrong reason. That is why the number
looked plainly wrong rather than merely suboptimal: nothing in the ranking had ever looked at it.

## Notes

- The whole point of this banner is that the fleet has to bridge somewhere and then travel, so jumps
  are the cost being minimised. Lightyears only matter for the bridge itself, which the in-range
  filter already handles.
- Ranking by BFS per candidate is not affordable: the candidate set is every system within 6 ly of
  staging. One BFS outward from the target, stopping at the first ring that contains an in-range
  system, gives the same answer in a single pass.
- `update_rescue_range` is already cached on `(staging, target)`, so cost per recompute is not
  frame-critical, but it does run on the UI thread.
- Behind `fc-rescue`. `cargo test --workspace` does not compile it.

## How to verify

`cargo test --bin eve-spai --features fc-rescue rescue_range`

The fix is wrong if the banner stops appearing. When nothing in titan range can reach the target at
all, the FC still needs to see that the target is out of range and that there is no route, so a
system must still be named rather than the warning disappearing.
