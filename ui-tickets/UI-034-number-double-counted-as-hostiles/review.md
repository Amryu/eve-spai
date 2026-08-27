# UI-034 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-034-numbers-double-counted`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none, done inline; the ticket says the code is delicate and it is |
| **Patches rejected on review** | 1, see "What I did not do" |
| **App code changed** | 40/4 lines in `intel.rs`, one function plus one new helper |
| **Harness code changed** | 0, this is not a layout defect |
| **Tests** | 476 to 479, and 503 to 506 with `fc-rescue` |
| **Follow-ups** | none |

## What changed

Two guards in `parse_count`, both placed so they run *before* the qualification test rather than
after it. That placement is the whole fix: the old guards only ever ran on numbers nothing else had
claimed, which is exactly the set that was already safe.

**Units.** `MAGNITUDE` became `UNIT_WORDS` and grew time and distance units alongside the ISK
magnitudes. A number whose following word is one of them is a quantity of something else. This
covers the ESS timer completely, because `parse_time_left` only reads `M:SS` (already rejected as
too long to be a count) and `N <time unit>`.

**Names.** New helper `number_in_pilot_name`. When the word before a bare number is capitalised, is
not a system, is not a ship, and the two together appear inside a pilot name that was actually
detected, the number is parked in `name_number_skips` instead of counted.

Parking rather than dropping matters. `app.rs:17492` adds the number back as a ship count if ESI
says the candidate is not a real character, so `Trinity 5` staying out of the count is provisional
and self-corrects, which is the same "let resolution decide, do not guess" shape the rest of the
pilot handling uses.

## What I did not do

The first version of the name guard reused the existing condition directly: previous word is a
name, so skip the number, regardless of qualification. It is two lines shorter and it breaks
`ESS 5 reds`. `ESS` is uppercase, alphanumeric and not a system, so `name_part` accepts it, and a
message that plainly says five reds would have shown no count at all.

Requiring the pair to appear in a **detected pilot name** is what separates the two cases. No pilot
`ESS 5` is ever produced there, while `Trinity 5 red` does produce `Trinity 5`. That check is the
reason the guard can be moved in front of the qualification test at all.

I also considered giving `parse_isk` and `parse_time_left` index-returning variants and passing the
claimed word positions into `parse_count`, which is the airtight version. All three functions index
the same `text.split_whitespace()`, so it would work. It needs `ess_ctx` moved above the
`parse_count` call and both parsers restructured, and the unit lookahead already covers everything
they can claim. Not worth reordering this function for. The note is in the ticket if a case ever
turns up that the lookahead misses.

## How the tests were proven to have teeth

Each guard reverted on its own, against a copy of the file kept aside rather than through git:

- Unit list back to ISK magnitudes only: `ess_timer_is_not_a_hostile_count` and
  `distance_is_not_a_hostile_count` fail, `number_in_a_pilot_name_is_not_a_hostile_count` passes.
- Name guard deleted: only `number_in_a_pilot_name_is_not_a_hostile_count` fails.

Each test also asserts the counts that must survive, so a future over-broad guard fails here rather
than silently blanking real counts. `ESS 5 reds` is in that list specifically because it is the case
the rejected patch broke.

## Evidence

No screenshots. The defect is in parsed data, not layout, and an intel-card scene renders whatever
count the fixture carries, so a PNG would prove nothing about the parser. The before and after
tables in `ticket.md` are `analyze` output and are the real evidence. Every wrong count in that
table is now `None`; every count in the must-stay table is unchanged.

## Residual risk

The unit list is a word list, so a unit nobody wrote down is still countable. The single letters `s`
and `h` are the loosest entries: a message ending `... 5 s` reads as five seconds now. Both need a
bare number in front and a count keyword or ship beside it to have been miscounted before, so the
list is strictly better than what it replaced, but it is a list.

`number_in_pilot_name` inherits whatever the name detector decided. A number is only spared when it
sits inside a name the parser emitted, so a missed name is still a miscount, and a spurious name
with a capitalised lead word can still swallow a real count. The `name_number_skips` round trip
limits the damage: if ESI does not recognise the candidate, the number comes back.
