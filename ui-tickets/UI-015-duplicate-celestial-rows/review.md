# UI-015 review cycle

**Status:** Fixed and verified
**Wave:** 6 (paired with UI-013 on `render_ping`, no region overlap)
**Branch:** `fix/ui-015-duplicate-celestials`

## The rule

`celestial_key(name) -> Option<CelestialKey>` with `enum CelestialKey { Star, Planet(i64),
Moon(i64, i64) }`. A `celestials` chip is skipped only when its key equals the `near_celestial`
key and that key is `Some`. Lowercased first, since case never decides anything in this codebase.

1. `"sun"`, `"star"`, or ending `" - star"` gives `Star`.
2. Contains `" - moon "` gives `Moon(p, m)` from the last token of the head and the first of the
   tail. Covers `"Planet VI - Moon 3 - Blood Raider Chemical Laboratory"` and the SDE form
   `"Jita IV - Moon 4 - Caldari Navy Assembly Plant"`.
3. Starts `"moon "` gives the parser's own `"6-3"` shorthand as `Moon(6, 3)`.
4. Any other name containing `" - "` gives `None`, which deliberately refuses
   `"Jita IV - Asteroid Belt 1"` so it cannot key as `Planet(4)` and swallow a `"Planet IV"` chip.
5. Otherwise two or more tokens with a final arabic or roman token gives `Planet(n)`.

## Why this shape

This was the riskiest ticket in the set. A false merge hides a chip, and a hidden chip can mean a
pilot not knowing hostiles sit at a *different* moon, which is worse than showing a duplicate.

Stricter is impossible: `==` on the raw strings never fires. `near_celestial` is an SDE name while
`celestials` entries are parser labels, and `detect_celestials` (`intel.rs:3150`) already folds
`"Planet VI - Moon 3"` into `"Moon 6-3"`, so the two sides are structurally comparable but textually
different by construction.

Looser was refused outright: no token overlap, no edit distance, no prefix matching. Every merge is
integer equality on indexes, and a moon needs both indexes. Anything unindexable falls through to
`None` and both chips render, which is the safe failure direction.

## Adversarial review

The agent's 7 tests cover the obvious must-not-merge cases. I added two more tests attacking the
rule directly, because "conservative" is a claim to be tested rather than accepted:

| Attack | Result |
|---|---|
| `Moon 6-3` vs `Moon 30` prefix collision | no merge |
| `Moon 6-30` vs `Moon 3`, and `Moon 60-3` vs `Moon 3` | no merge |
| Roman-looking system token `7-K5EL` | `None`, digits keep it out of the roman path |
| `MJ-5F9 IV` | `Planet(4)`, and `MJ-5F9` alone is `None` |
| Belts, `Asteroid Belt` vs `Jita IV - Asteroid Belt 1` | no merge |
| `Planet VI` vs `Planet VI - Moon 3` | no merge, a planet never swallows its own moon |

All pass. `roman_to_int` returns `None` on any non-roman character, so junk tokens cannot become
indexes.

**One real limitation, pinned as a test rather than left implicit:**
`review_planet_key_ignores_the_system` asserts `same("Jita IV", "Amarr IV")` is true. The key carries
no system, so two planets with the same index collide. Both fields come from one report, so it takes
a report naming two systems' planets to bite. Documented so it is a known trade rather than a
surprise.

## Tests

- `mod celestial_key_tests` in `app.rs`: 9 tests, 7 from the agent plus my 2 adversarial ones.
- `uitest_intel_row_folds_the_duplicate_celestial`: the torture card draws exactly one chip
  containing "Chemical Laboratory", and it still carries "0 km".
- `uitest_intel_row_keeps_a_second_celestial`: new fixture with near `Planet VI - Moon 3` plus
  celestial `Moon 6-4`, both chips required. This is the safety test.

The surviving chip is the `near_celestial` one, which carries the distance and the full-name hover.
The key is computed inside the `dm <= 15_000_000.0` guard, so a far-away kill still shows the
`celestials` chip.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 416 passed, 2 ignored (+32) | **427 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 17 passed | 19 passed |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Screenshots

- `after/intel_row_torture_full.png`: one cyan celestial chip,
  `"Moon 6-3 - Chemical Laboratory  0 km"`. The `"Planet VI - Moon 3 - Blood Raider Chemical
  Laboratory"` chip is gone, the freed room pulled Ragnarok up into the ship row, and the card is
  one row shorter. Counts, ISK, pilots, flags and footer unchanged.
- `after/intel_row_two_celestials.png` (new): both chips render, `"Moon 6-3 - Chemical Laboratory
  0 km"` and a separate `"Moon 6-4"`. This is the case the rule must never merge.
- `after/alert_window_torture.png`: the third card packs tighter and one more pilot row fits.

## Known misses, from the agent, all in the safe direction

- A `celestials` planet chip still renders beside a near moon chip. Correct by the rule; the real
  duplication there is inside `celestials` itself and out of scope.
- Belts never merge: the parser label has no belt index.
- A moon whose SDE name lacks `" - Moon "` will not merge.
