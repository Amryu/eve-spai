# UI-035 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-035-rescue-jump-off-ranking`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none, done inline |
| **Patches rejected on review** | 0 |
| **App code changed** | 68/10 lines: a new `Systems::nearest_matching`, and the ranking moved out of `update_rescue_range` into `best_jump_off` |
| **Harness code changed** | 0, the defect is in a graph search, not layout |
| **Tests** | 506 to 510 with `fc-rescue`, 479 to 480 without |
| **Follow-ups** | none |

## What changed

`Systems::nearest_matching(from, max_jumps, ok)` in `geo.rs` walks the graph outward from `from` ring
by ring and returns the first ring containing a match, plus every match in that ring. `best_jump_off`
in `app.rs` calls it from the **target**, with `ok` being "in titan range of staging", and breaks
ties inside the winning ring by lightyears to the target.

Searching from the target rather than per candidate is what makes this affordable. The candidate set
is every system within 6 ly of staging, which in Delve is not small, and a BFS each would be far
worse than the scan it replaced. One outward walk answers it in a single pass, and it stops at the
first ring that hits rather than exploring the whole graph.

Returning the whole ring rather than one system is deliberate. The reported case is two systems one
jump out, and picking by iteration order would make the banner flip between them for no visible
reason. The lightyear tie-break is the old ranking demoted to where it is actually correct: among
equals, the shorter bridge is better.

## Which route metric ranks

Bridge-aware, matching `Systems::jumps`, which is what the banner already reports first as "Ansiblex
jumps". The rescue map preset keeps the ANSI bridge layer on and strips nearly everything else, so
the tool already treats the bridge network as part of this operation. Gate jumps are still shown
beside it, unchanged, so an FC who cannot bridge the fleet sees that number too.

This is the one judgement call in the fix. If capitals in the rescue fleet cannot take the bridges,
the ranking should key on `jumps_gates_only` instead: it is a one-line change in `nearest_matching`,
and the fixture tests would need a bridge edge added to tell the two apart.

## How the tests were proven to have teeth

`best_jump_off` reverted to the old lightyear ranking, against a copy of the file kept aside rather
than through git. `jump_off_is_the_fewest_jumps_out_not_the_nearest_on_the_map` and
`ties_at_the_same_jump_count_go_to_the_nearer_one` both fail;
`unroutable_target_falls_back_to_the_nearest_on_the_map` still passes, correctly, since the fallback
*is* the old behaviour.

The fixture mirrors the report: staging at 0 ly, the stranded capital at 10 ly so the warning is
live, `ZH-GKG` at 5.9 ly and ten gates out, `NEAR-A` and `NEAR-B` at 3.0 and 3.5 ly and one gate out.
The first test asserts up front that `ZH-GKG` really is the lightyear-nearest, so the fixture cannot
rot into one where both rankings agree and the test passes for the wrong reason.

## Evidence

No screenshots. The banner's layout was never wrong, only the system it named, and a rescue scene
renders whatever `RangeWarning` the fixture carries. The graph fixture and its assertions are the
evidence.

Not verified in the running app: this is behind `fc-rescue` and the user asked for no restarts. The
reported case is reproduced in the fixture by construction, not by replaying real SDE coordinates,
so the real 5J4K-9 answer is confirmed by the logic and the tests rather than by observation.

## Residual risk

`MAX_JUMPS` is still 40. A target more than 40 jumps from everything in bridge range now falls back
to the lightyear pick rather than reporting nothing, which is the old wrong answer shown in the one
situation where no right answer exists.

The tie-break makes the pick stable for a fixed graph, but adding or removing a bridge can move the
winning ring and change the named system. That is correct behaviour, and it will still look abrupt
to an FC watching the banner while sov changes.
