# UI-037 &mdash; An intel card's jump distance is measured from the selected character, not the one the alert fired on

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `intel_row`'s jump chip, `SpaiApp::player_system` |
| **Reported by** | user |

## Symptom

With more than one character authenticated, a card reports how far the report is from whichever
character is picked in the top-bar combo. That is not the number the alert used. The alert engine
already measures from every alert-enabled character and fires on the nearest, so a report that
alerted at one jump sits on a card reading four, and nothing on screen says either number belongs
to a particular character.

The user's words: "it currently shows the jump range from the currently selected one. This can be
confusing."

## Measured

Fixture card, `1DQ1-A`, with Scout Alt one jump away and Amryu (selected) four.

| | Card reads | Portraits | What fired the alert |
|---|---|---|---|
| Before | `4j` | none | Scout Alt at 1 jump |
| Expected | `1j` and `4j` | nearest, then selected | Scout Alt at 1 jump |

`before/intel_row_two_characters.png` is the whole of it: one number, no owner.

## Cause

Two origins for one distance.

- The card: `App::player_system()` (`app.rs:11210`), `p.locations.get(&self.active_character)`, a
  single system, threaded into `jumps_from_you` at every card site.
- The alert: `AlertEngine::evaluate`'s `char_systems` closure (`app.rs:17163`) collects every
  character not in `Settings::intel_disabled_chars`, drops docked ones when
  `Settings::alert_only_undocked` is on, and `min_jumps_from` (`app.rs:18387`) takes the minimum.

Nothing was missing from the data. `esi::Player::locations` (`esi.rs:13`) already holds every
character's system, refreshed every 20s by `spawn_location_poller` (`esi.rs:23`), with offline
characters deliberately absent (`esi.rs:57`). The card just never looked at more than one entry.

## Notes

- **Cost is the constraint.** Card distances are recomputed per card per frame with no cache:
  UI-026 measured 21 us/card in the home region and 171 us/card map-wide over 250 cards, and the
  `max_jumps` filter (`app.rs:5867`) pays it for every match rather than every visible card. A BFS
  per character per card multiplies that by the roster and reproduces the 80ms frame hitch UI-026
  was filed against.
- The overlay subprocess renders the same `intel_row` and holds neither the roster nor anyone's
  location, so anything new has to cross IPC as a verdict, not as inputs. UI-029 set that precedent
  for `JumpVia`, including the `AlertPush` resize that stops a short vector shifting values onto
  the wrong cards.
- UI-002 owns the jump column: `from_you == None` must render nothing, because an unconditional
  padded label was a 34x28 invisible click target. UI-026 owns the bridge mark as a separate
  conditional label.
- `intel_disabled_chars` keys by character name; a portrait keys by id. A renamed character loses
  both, which is pre-existing but becomes visible once a portrait is drawn.

## How to verify

`cargo test --bin eve-spai uitest_intel_card`, and
`cargo test --bin eve-spai uitest_screenshots_char_attribution -- --ignored` against `before/`.

The fix is wrong if it reaches the nearest number by widening the single origin, i.e. by making
every card show a minimum with no attribution: the user has to be able to see WHICH character each
number belongs to, and the selected character's own distance has to stay on screen. It is also
wrong if it puts a badge on a single-character card, if it grows the row height (a framed button
floors to `interact_size.y`, which is the 26px-for-15px-of-ink defect UI-027 fixed), or if it costs
a graph walk per card.
