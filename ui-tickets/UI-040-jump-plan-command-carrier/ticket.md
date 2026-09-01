# UI-040 Jump Plan mode has no command carrier

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `jumproute::SHIP_CLASSES`, `jump_plan_ui` (app.rs) |
| **Reported by** | user |

## Symptom

The Jump Plan ship picker offers five classes: Capital (Dread / Carrier / FAX),
Supercarrier / Titan, Black Ops, Jump Freighter, Rorqual. There is no command carrier, so
a command carrier route has to be planned as a plain capital, which is short by 0.5 ly
base and 1 ly at maxed skills.

## Measured

| Class | base_ly | at JDC V |
|---|---|---|
| Capital (Dread / Carrier / FAX) | 3.5 | 7.0 |
| Command carrier (missing) | 3.75 | 7.5 |
| Black Ops | 4.0 | 8.0 |

Planning a 7.5 ly command carrier hop as a capital drops it from the jump graph entirely,
because `shortest_path_pref` connects two systems only when their distance is within
`max_range_ly`. The route is not merely mis-costed, it is not found.

## Cause

`app/src/jumproute.rs:30`, `SHIP_CLASSES` is a fixed table and the hull is not in it.

## Notes

- Saved routes store the picker index (`app.rs:11756`, `r.ship.min(SHIP_CLASSES.len() - 1)`),
  and the header comment on the table says as much: "Rorqual sits last so saved routes keep
  their class index". A new class appends, it does not sort into place.
- `jump_dockable_ids` tests `self.jump_ship == 1` for the supers-only docking rule. That
  index must keep meaning Supercarrier / Titan.
- Fuel is 3000/ly with no fuel or fatigue role bonus, per the user. Range is the only
  attribute the ticket measures.
- `map::JUMP_RANGES` is a separate table driving the map's range rings, deliberately left
  alone: it is four coarse bands and a 7.5 ring between the 7.0 and 8.0 ones would print
  as "8 ly" under the ring label's `{ly:.0}` format.

## How to verify

`cargo test --bin eve-spai jump_plan` plus the picker screenshot. The fix is WRONG if the
new class is inserted anywhere but the end of `SHIP_CLASSES`, if `max_range_ly` for it at
JDC V is not 7.5, or if the supers docking rule starts matching it.
