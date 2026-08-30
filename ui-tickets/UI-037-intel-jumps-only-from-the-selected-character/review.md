# UI-037 review cycle

**Status:** Fixed and verified
**Branch:** `feat/ui-037-nearest-alerting-character`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none for the fix, done inline; 3 explore agents and 1 plan agent for the design |
| **Patches rejected on review** | 0 |
| **App code changed** | 836/23 lines across `app.rs`, `geo.rs`, `ipc.rs`, `overlay.rs` |
| **Harness code changed** | 333/0 lines: 4 scenes, 3 fixtures, 8 assertions |
| **Tests** | 512 to 537 passing, 4 to 7 ignored |
| **Card reads** | `4j` alone to `1j` + `4j`, each under its own portrait |
| **Distance cost** | one BFS per card per frame to one ball per character per move |
| **Follow-ups** | none filed; two things deliberately left out, below |

## What changed

`CharHop` and `CardChars` (`app.rs`) carry per-character distances for one card. `CharRings` holds
one BFS ball per alert-enabled character, built once per view by `SpaiApp::char_rings()` and read
per card by `CharRings::card(target)`, which is N hash lookups. `alert_candidate` is the single
definition of "alerts enabled", called by both the card and `AlertEngine::evaluate`'s `char_systems`
closure so the two cannot drift.

`geo::Systems::distances_from` / `gate_distances_from` walk the graph once from an origin.
`distance_ball` memoizes them per graph `Arc` and origin, the same invalidation story `ViaMemo`
already relies on: `Systems` is only mutated before it is wrapped, so a bridge edit replaces the
`Arc`.

`intel_row` takes `chars: &CardChars`. With hops it draws a badge and number per slot; without, it
falls through to the existing path byte for byte, which is what all 17 call sites and every
existing scene get. Full cards carry the nearest and the selected, the selected dimmed; compact
cards carry only the nearest and put the rest in the roster the badge opens.

`ipc::AlertMsg` gains `#[serde(default)] chars: Vec<CardChars>`, index-aligned with `feed`, because
the overlay subprocess holds neither the roster nor anyone's location. `AlertConfig` gains the
name-and-id roster, since the engine thread has no store handle and the overlay needs ids for
portraits.

## Cost

The reason for balls rather than a walk per character per card, using UI-026's measurements
(21 us/card home region, 171 us/card map-wide, 250 cards):

| | per move, per character | per card | 250-card frame |
|---|---|---|---|
| Before, one character | 0 | one BFS | up to 43ms in the `max_jumps` filter |
| A walk per character per card | 0 | N x 171us | 213ms at five characters |
| Balls | 2 BFS | 2N lookups, ~0.3us | ~75us |

A character's ball is rebuilt only when it moves, which `spawn_location_poller` bounds to once per
20s. A k-space ball is ~90KB and the memo caps at 16.

## Two bugs the tests found

**The badge flipped the card to raw text.** `intel_row` toggles the raw view on any unclaimed
background click (`app.rs:23684`, `bg_click && !consumed`). The portrait button did not claim its
click, so opening the roster also flipped the card underneath it. `char_jump_slot` now returns
whether it took the click and the caller ORs it into `consumed`. Found by
`uitest_intel_card_badge_opens_the_character_roster`, which saw the card collapse to its raw line
instead of showing a menu. This would have shipped invisibly, since the menu covers the card it
just changed.

**The alert window closed while the roster was open.** Auto-dismiss only pauses on
`ui_contains_pointer` (`app.rs:20991`), and a menu opens in its own `Area` outside that `Ui`.
`hovered |= egui::Popup::is_any_open(&ctx)` fixes it, following the two places that already force
`hovered` for open popups.

## Teeth

- `a_ball_answers_exactly_what_jumps_would` compares every ordered pair against `jumps` and
  `jumps_gates_only` across five caps and two graphs. Reordering the record and the transit check
  in `ball` fails it with `zarzakh: ball 1 -> 30100000 at cap 1, left: None, right: Some(1)`. That
  ordering is not obvious: `bfs_jumps` answers `n == to` *before* testing `is_no_transit`, so a
  route may end at Zarzakh and never pass through it. This test is what licensed reusing the ball.
- `uitest_intel_card_compact_shows_only_the_nearest` fails with two numbers if the `!compact` guard
  goes; `uitest_intel_card_keeps_the_badge_when_nearest_is_selected` fails with two identical
  numbers if the `i != 0` guard goes; `uitest_intel_card_draws_no_badge_for_one_character` fails if
  a badge is drawn unconditionally.
- `uitest_intel_card_jump_column_holds_its_x` pins UI-002's column across a card with a second slot
  and one without.
- `the_hundred_card_trim_cuts_every_vector_by_the_same_amount` pins the shift bug the `via` comment
  only described. The merge was extracted from the overlay message loop into `push_reports` to make
  it reachable at all.
- `min_jumps_from`, `rule_matches`' candidate set and the docked rule had no tests before this; the
  `char_rings_tests` module covers them now.

## Screenshots

`before/intel_row_two_characters.png`: one `4j` chip, nothing saying an alt sits one jump from that
system. `after/`: a portrait, `1j`, a second portrait, a dimmed `4j`. The narrow pair shows the
compact card keeping only the nearest. `intel_row_two_characters_bridged` shows the bridge glyph on
the one character whose trip it shortened, which a single shared `JumpVia` could not express. The
grey squares in the harness renders are `StubImages` standing in for portraits, which do not load
headlessly.

## Deliberately not done

- **The roster does not switch the active character.** It is not what was asked, `active_character`
  has exactly one writer today, and the overlay would need a new `OverlayToMain` variant and a
  round trip. Worth its own ticket if wanted.
- **`min_jumps_from`'s hardcoded `50`** stays, rather than `JUMP_SCAN_CAP`. It is on the alert path
  and had no tests; it has tests now, so the constant can move in its own commit.

## Residual risk

The alert window is not virtualized and renders up to 100 cards, so two badges there is up to 200
more AccessKit nodes and menu ids, where the intel feed culls to roughly 20 visible. Not measured
under load. If it bites, the second slot can be dropped there the way it already is on compact
cards.
