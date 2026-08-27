# UI-034 &mdash; Intel cards show a hostile count taken from a number that means something else

| | |
|---|---|
| **Severity** | High |
| **Status** | Fixed, see `review.md` |
| **Region** | `parse_count` in `app/src/intel.rs` |
| **Reported by** | user |

## Symptom

Intel cards show wrong hostile counts. A number that another parser already read as an ESS hack
timer, an ESS bank amount, a distance, or part of a pilot name is counted a second time as hostiles.

## Measured

Parser output before the fix, from `analyze` with the test system and ship fixtures:

| message | shown as | the number actually is |
|---|---|---|
| `ess reds 5 min Rancer` | **5 hostiles** | the 5 minute hack timer (`ess_time` = `5m`) |
| `ess hostiles 2 min left in Rancer` | **2 hostiles** | the timer (`ess_time` = `2m`) |
| `ess neuts 45 mins left Rancer` | **45 hostiles** | a 45 minute duration |
| `reds 30 seconds out Rancer` | **30 hostiles** | an ETA |
| `reds 20 km off gate Rancer` | **20 hostiles** | a range off the gate |
| `hostiles 100 au out Rancer` | **100 hostiles** | a range |
| `Trinity 5 red in Rancer` | **5 hostiles** | part of the pilot name `Trinity 5` |
| `Bob 7 neut in Rancer` | **7 hostiles** | part of the pilot name `Bob 7 neut` |

ISK amounts and celestials were already handled: `ess bank 500m 6 neuts` reads 6, and
`planet 4 reds` reads no count.

## Cause

`app/src/intel.rs`, `parse_count`. A number is counted when it is *qualified*, and one of the
qualifiers is a red/neut/hostile keyword or a ship word sitting on either side of it. Nothing
checks whether some other parser has a stronger claim on the same number.

Two specific holes:

1. The lookahead that rejects `334 million` covers ISK magnitudes only (`MAGNITUDE`). A number
   followed by a time unit (`min`, `mins`, `seconds`) or a distance unit (`km`, `au`) is not
   rejected, so a nearby `reds` promotes it to a count. `parse_time_left` reads the same number as
   the ESS timer, so a single number lands in two fields of the same card.
2. The existing name guard, which parks a `Lead N` pair in `name_number_skips` for ESI resolution
   to settle, is gated on `!qualified`. `Trinity 5 red` puts a count keyword after the number, so
   the guard never runs and the name's own digits become the hostile count.

## Notes

- The count is derived once by `parse_count` and re-derived in `app.rs` from stored components after
  ESI resolution, so a bad `count_extra` persists across resolution. It has to be right at parse
  time.
- `name_number_skips` is the deferred-decision path, not a drop: `app.rs:17492` adds the number back
  as a ship count if resolution says the candidate is not a real character. A name-attached number
  belongs there rather than being discarded.
- `parse_count`, `parse_isk` and `parse_time_left` all index the same `text.split_whitespace()`, so
  word positions are comparable between them if that is ever needed.

## How to verify

`cargo test --bin eve-spai intel::`, plus the table above through `analyze`.

The fix is wrong if it also suppresses a real count. These must keep working, and each one is a
number sitting next to a word that the fix might over-read:

| message | must stay |
|---|---|
| `ESS 5 reds Rancer` | 5 (capitalised `ESS` in front of it, but no pilot `ESS 5` is detected) |
| `ESS 5:00 3 reds Rancer` | 3 (a real count beside a real timer) |
| `ess 45 min reserve 3 reds Rancer` | 3 |
| `ess bank 500m 6 neuts Rancer` | 6 |
| `Rancer gate 5 reds` | 5 |
| `belt 3 neuts Rancer` | 3 |
| `reds 6 Rancer` | 6 |
| `5 Loki Rancer` | 5 |
