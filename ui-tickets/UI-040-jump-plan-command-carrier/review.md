# UI-040 review cycle

**Status:** Fixed and verified
**Branch:** `ui-040-jump-plan-command-carrier`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | one session, no fix agent dispatched (one table row plus tests) |
| **Patches rejected on review** | 0 |
| **App code changed** | +5 / -1 excluding tests and the harness |
| **Harness code changed** | +19 (one scene builder, two scenes) |
| **Suite** | 562 to 564 |
| **Max range at JDC V** | 7.0 ly (planned as a capital) to 7.5 ly |
| **Follow-ups** | none |

## What changed

One row appended to `jumproute::SHIP_CLASSES`: `base_ly: 3.75`, `fuel_per_ly: 3000.0`, no
fuel or fatigue role reduction. `max_range_ly` multiplies by `1 + 0.20 × JDC`, so JDC V
gives 7.5 ly and an unskilled pilot 3.75, which is what the user specified. Nothing else in
the planner needed touching: the picker iterates the table, the jump graph is built from
`max_range_ly`, and the docking rule keys off `jump_ship == 1` (Supercarrier / Titan), which
a class appended at index 5 does not disturb.

Fuel and the role bonus were not in the request. Asked, answered as carrier-standard 3000/ly
with no bonus, and the test pins that a command carrier and a capital cost identically over
the same distance, so a later correction fails loudly instead of drifting.

`map::JUMP_RANGES` was deliberately left alone. It drives the four coarse range rings on the
map, labelled `{ly:.0}`, so a 7.5 ly ring between the existing 7.0 and 8.0 ones would print
"8 ly" next to the black ops "8 ly" and sit about two pixels from both. The planner is where
the class matters.

## Rejected

Sorting the table by range, which reads better in the picker and silently repoints every
saved route: `app.rs:11756` restores `r.ship` as an index into `SHIP_CLASSES`. The header
comment already warned about this ("Rorqual sits last so saved routes keep their class
index"); it now says append, and `existing_class_indices_are_stable` enforces it.

Deriving the class from the character's current hull over ESI. Out of scope, and the picker
exists because people plan routes for ships they are not sitting in.

## Teeth

`before/tests-fail-without-the-class.txt` is the suite with the `SHIP_CLASSES` row removed
and the tests left in place. Two of the three new tests fail:

- `maxed_ranges_match_game`, `[7.0, 6.0, 8.0, 10.0, 10.0]` against the expected trailing 7.5
- `command_carrier_reaches_where_a_capital_cannot`, on the lookup by name

`existing_class_indices_are_stable` passes in both states by design. It is a guard against a
future edit, not an assertion about this one.

The interesting half of `command_carrier_reaches_where_a_capital_cannot` is the 7.2 ly hop:
`shortest_path_pref` returns `None` at a maxed capital's 7.0 and the direct path at 7.5. The
missing class was not costing a route wrong, it was hiding the route.

## Screenshots

`before/jump_plan_capital.png` is what a command carrier pilot had to plan with, the panel
reading "Capital (Dread / Carrier / FAX)" and 7.0 ly beside JDC 5.
`after/jump_plan_command_carrier.png` is the same panel on the new class, reading 7.5 ly. The
range readout next to the skill fields is the only thing that moves, which is the whole
visible surface of this change with the combo closed.

Both are new scenes (`jump_plan_capital`, `jump_plan_command_carrier`). The Jump Plan panel
had no harness coverage before this ticket.

## Residual risk

The class list is still a hand-maintained table transcribed from the SDE, so a hull rebalance
goes stale silently. `maxed_ranges_match_game` is the only thing standing between the planner
and a wrong number, and it asserts what someone typed, not what the SDE says.

The picker is closed in both scenes, so the list itself is unphotographed. Opening a
`ComboBox` in kittest needs a click plus a second pass, which the `all()` scenes cannot do
(they render once); the class list is covered by the unit tests instead.
