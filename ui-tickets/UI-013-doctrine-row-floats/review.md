# UI-013 review cycle

**Status:** Fixed and verified
**Wave:** 6 (paired with UI-015 on `intel_row`, no region overlap)
**Branch:** `fix/ui-013-doctrine-row`


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 10.2 min across 1 round, 45 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 25/18 lines (added/removed), excluding the harness |
| **Harness code changed** | 125/1 lines |
| **Suite** | 427 to 430 passing |
| **Follow-ups** | UI-018 |

## The change

`render_ping`, Fleet arm, doctrine row only. `ui.horizontal_wrapped(..)` becomes
`ui.allocate_ui_with_layout(size, Layout::left_to_right(Align::Center).with_main_wrap(true), ..)`
with `size.y = 0.0`, and the whole block is guarded by
`if doctrine.is_some() || !doctrine_url.is_empty()`.

## Root cause, and a dead end worth recording

`horizontal_wrapped` is exactly that layout, except it hardcodes
`initial_size.y = spacing().interact_size.y` (26.0 in this theme) on the assumption the row will
hold something interactive. This row only ever holds text and links, so the floor is 11px of dead
air.

The agent first tried `ui.spacing_mut().interact_size.y = 0.0` *inside* the closure and **measured
zero change**, because egui reads `interact_size` off the parent before creating the child ui. It
reported the dead end rather than quietly moving on, which is worth having: that is the obvious fix
and it does nothing.

The guard addresses a second symptom of the same cause: a Fleet ping with neither a doctrine nor a
configured URL previously left an empty 19px row behind.

## Measurements

Allocated row rects, `ping_fleet` at 520x320:

| row | before | after |
|---|---|---|
| FC | h 15.0 | unchanged |
| Formup | h 15.0 | unchanged |
| Comms | h 27.0 | unchanged, holds the Join Mumble button |
| **Doctrine** | **h 26.0** | **h 15.0** |

Ink-band gaps: `14, 11, 7, 15, 20, 12` before, `14, 11, 7, 9, 15, 12` after. Card is 11px shorter,
and 21px shorter with no doctrine at all, which is the 15px row plus 6px spacing, so no phantom row
remains.

## Doctrine-link-present case

The ticket flagged that the existing fixture passes an empty `doctrine_url`, so only one of the two
cases was covered. Both now measure identically at h 15.0, with the chip on the same top edge as the
label. Verified with two new scenes (`ping_fleet_doctrine_link`, `ping_fleet_no_doctrine`) so both
go through `uitest_layout`'s overlap and escape checks, plus three assertions.

**Teeth confirmed.** Restoring only the height floor while keeping the scenes fails with
`doctrine_url "": Doctrine row is 26.0px against 15.0px for FC`, exactly as reported. The agent
separately confirmed `uitest_ping_without_a_doctrine_leaves_no_row` fails on the true unfixed code
with `dropping the doctrine saved 13.0px, not the 32.0px its row plus spacing occupies`.

`uitest_ping_doctrine_link_shares_the_doctrine_row` passes before and after by design: it guards
against a future fix splitting the chip onto its own row.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 427 passed, 2 ignored (+32) | **430 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 19 passed | 22 passed |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Screenshots

- `after/ping_fleet.png`: Doctrine sits tight under Comms at the same leading as FC and Formup. It
  previously floated with roughly 6px of dead air above and below the glyphs. Comms is now the only
  tall metadata row, which reads as intentional because it is the only one holding a button.
- `after/ping_fleet_doctrine_link.png`: `Doctrine: Muninn  Doctrines ↗` on one line, chip aligned
  with the label, same row height as the no-chip case.
- `after/ping_fleet_no_doctrine.png`: Comms runs straight into the description with no gap.
- `after/ping_window_mixed.png`: same card in the overlay, 11px shorter, nothing clipped. The Plain
  card below is untouched.

## Related issue found, ticketed as UI-018

The remaining 15px gap at Doctrine to description comes from `render_ping_body`, which wraps every
line in its own `horizontal_wrapped` and so allocates 26px for 15px of ink. Same class of bug, but
it is shared with Plain pings and the jabber body renderer at `app.rs:23707`, so changing it here
would have reached well outside this ticket's region.
