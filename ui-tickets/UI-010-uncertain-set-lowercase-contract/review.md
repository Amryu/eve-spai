# UI-010 review cycle

**Status:** Fixed and verified
**Wave:** 7 (paired with UI-014 on `render_ping`)
**Branch:** `fix/ui-010-uncertain-newtype`


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 5.1 min across 1 round, 28 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 52/8 lines (added/removed), excluding the harness |
| **Harness code changed** | 65/9 lines |
| **Suite** | 431 to 433 passing |
| **Follow-ups** | none |

## The change

`pilot::UncertainPilots(HashSet<String>)` with a private tuple field, so only `pilot.rs` can reach
the raw set. Every way in normalizes:

- `impl<S: AsRef<str>> FromIterator<S>` lowercases each entry.
- `Deserialize` is hand-written to route through `FromIterator`, so an IPC payload carrying
  display-cased names normalizes too.
- `Serialize` is `#[serde(transparent)]`, so the wire format stays a plain JSON array of strings and
  old and new overlay subprocesses interoperate.
- `contains(&self, name: &str)` lowercases the query.

## Why the newtype

Of the three options in the ticket this is the only one that removes the trap rather than
documenting it.

**Lowercasing inside `intel_row`** would build a fresh normalized set per card, N string allocations
per card on a virtualized feed, and would leave the IPC and `AlertWindowState` copies still
mis-casable.

**Rename plus doc comment** was already effectively tried and already failed. `fixtures.rs` carried
an explicit "Keyed by LOWERCASED name" comment, written when I first hit this, and the feature still
went unrendered afterwards. Documenting a trap does not stop people falling into it.

## Can a future caller still get it wrong?

Honestly: only inside `app/src/pilot.rs`, which is the one module that can name the private field.
From anywhere else the set is unconstructible except through `FromIterator`, `Default`, `Clone` or
`Deserialize`, all of which normalize. A future author adding a mutating method inside `pilot.rs`
could reintroduce it; nothing outside can.

## Allocation

Net reduction, which I did not expect. `contains` guards with `!self.0.is_empty()` before
`name.to_lowercase()`, so the common case, no pilot flagged anywhere in the feed, now does zero
allocation per pilot per card where the old code allocated unconditionally. When the set is
non-empty it is one `String` per pilot chip, the same as before. Construction no longer allocates a
second lowercase `String` per key.

## Tests

- `pilot::tests::uncertain_pilots_normalizes_every_way_in`: `FromIterator`, the serde round trip,
  deserializing a display-cased array, and `Default`.
- `uitest_intel_row_marks_uncertain_pilot_from_display_cased_set`: builds `IntelArgs` with
  `uncertain: ["Second Target"].into_iter().collect()`, the exact shape that used to render nothing,
  and asserts the chip carries the "?", that an unflagged pilot does not, and that clicking yields
  `IntelClick::PilotVerdict`.

The fix makes the broken state unrepresentable, so it cannot be asserted directly. The agent
verified the tests are real by temporarily changing `FromIterator` to `to_owned()`, reproducing the
old contract: both new tests failed with `no uncertain marker on the flagged pilot`, and both pass
with lowercasing restored. The 6 existing IPC round-trip tests still pass, which is what confirms
the wire format is unchanged.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 431 passed, 2 ignored (+32) | **433 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 23 passed | 24 passed |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Merge conflict, resolved by hand

Both wave-7 agents appended a test to the end of `scenes.rs`, so this patch needed `git apply -3`
and a manual resolve. Keeping both sides was the right answer, but the conflict boundary had
truncated UI-014's test before its closing `);` and `}`, which showed up as an unclosed delimiter
rather than as a silently dropped assertion. Restored, and both tests confirmed passing
individually.

Worth noting for the workflow: pairing two agents by *region of `app.rs`* does not stop them
colliding in `scenes.rs`, since every ticket now adds a test there. Recorded in CLAUDE.md.

## Screenshot

`after/intel_row_typical.png`: the "Second Target" chip still shows the amber "?" and the dark amber
fill, and "Hostile Pilot" stays plain. The default fixture now supplies display-cased names, so the
chip renders through the normalizing path rather than through a pre-lowercased set.

## Also updated

`CLAUDE.md`'s harness trap list documented this lowercase contract as a live trap. It is no longer
one, so the entry is gone.
