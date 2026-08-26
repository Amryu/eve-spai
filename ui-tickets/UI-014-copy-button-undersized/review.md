# UI-014 review cycle

**Status:** Fixed and verified
**Wave:** 7 (paired with UI-010 on `intel_row`, no region overlap)
**Branch:** `fix/ui-014-copy-button-size`


## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | 4.8 min across 1 round, 33 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 2/2 lines (added/removed), excluding the harness |
| **Harness code changed** | 25/0 lines |
| **Suite** | 430 to 431 passing |
| **Follow-ups** | UI-019 |

## The change

`ui.small_button(..)` becomes `ui.button(..)` for the Copy button in both ping arms, Fleet and
Plain. Two lines, no comment, the call reads for itself.

## Review

The right size came from the theme rather than a number. `small_button` zeroes vertical padding and
drops the `interact_size.y` floor set in `theme.rs:167-168`. A plain `Button` picks both back up and
lands at 68.2x27, which is exactly the height of Join Mumble in the same card. Nothing is hardcoded,
so it tracks any future spacing change.

Worth correcting from the ticket: I wrote "115x28" for Join Mumble from a rounded pixel
measurement. The real rect is 27px tall, and Copy now matches it exactly.

The header row did not grow. It was already floored at `interact_size.y`, so the taller button fills
space that was previously empty.

## Census, the ticket's stated acceptance check

| scene | before | after |
|---|---|---|
| `ping_fleet` | 17px Copy | **27px** Copy |
| `ping_fleet_doctrine_link` | 17px | 27px |
| `ping_fleet_no_doctrine` | 17px | 27px |
| `ping_plain` | 17px | 27px |
| `ping_window_fleet` | 17px | 27px |
| `ping_window_mixed` | 17px | 27px |

Copy is still the smallest target in those scenes, now at 27px against the 28px intel-chip floor,
and above the 18px nav-rail icons that dominate the `view_*` scenes.

## Test added

`uitest_ping_copy_matches_the_other_buttons` asserts Copy's height is within 1px of Join Mumble and
that Copy and the timestamp still share a centre line on the header row.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 430 passed, 2 ignored (+32) | **431 passed**, 2 ignored (+32) |
| `cargo test --bin eve-spai uitest` | 22 passed | 23 passed |

`cargo check --workspace --all-targets --all-features`: only the pre-existing warning at
`app/src/intel.rs:5605`.

## Screenshots

- `after/ping_fleet.png`: Copy is a proper bordered button carrying the same visual weight as Join
  Mumble, still right-aligned with the timestamp to its left.
- `after/ping_fleet.debug.png`: the hit rect is a full-height box spanning the header row rather
  than a thin sliver around the text, and does not overlap the timestamp label.
- `after/ping_window_mixed.png`: both cards' Copy buttons match, each centred against its timestamp.

Note the card in these renders is about 10px shorter than the `before/` capture. That is UI-013's
doctrine-row fix, already landed, not this change.

## Other `small_button` sites, ticketed as UI-019

Sixteen more call sites outside this ticket's region, listed in UI-019. Most are icon-only buttons
in dense toolbars where small is a defensible choice, so that ticket is an audit rather than an
assertion that they are all wrong.
