# WEB-002 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-002-snapshot-publisher`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 583 to 590 |
| **Follow-ups** | none |

## What changed

**`AlertEngine::build_alert_msg`.** `push_overlay_update` was one 180-line function that enriched the
feed and then sent it. The enrichment is now its own method and the push is a thin caller that adds
`secs`, `focus`, the hash comparison and the send. The overlay and the web view now read the same
assembled feed, so they cannot end up showing different things about the same card.

The one behavioural seam is deliberate: the old code gated the feed on
`enabled && any(rule.custom_window)`, because the overlay only wants a feed when a rule opens its
window. That gate is now a `gate` parameter. The overlay passes exactly what it passed before; the
web view passes `alerts_enabled()`, because a phone should see fired alerts whether or not the user
also wanted a desktop popup.

The hash is computed over the same nine components as before, in the same order, now read off the
built message. The suite stayed at 583 across the refactor, which is the evidence that nothing about
the overlay changed.

**`web::snapshot`.** One `Option` per pane, `None` meaning unchanged, each pane carrying the `rev` it
last changed at. `snapshot_since(n)` filters on that, so a reconnecting client asks once and gets
exactly what it missed. No patch format was written, on purpose: a patch format is a second thing to
get subtly wrong, and the panes are small enough that resending a changed one is cheaper than
reasoning about a diff.

The alert pane embeds `ipc::AlertMsg` verbatim rather than reshaping it. It already carries the feed
and every lookup, and `ipc.rs` has compatibility tests pinning it.

**`web::publish`.** A 500ms thread. Not the egui loop, because the whole point is that the phone
keeps working while the desktop window is minimized and egui parks. Not the alert daemon, which at
400ms already carries kill ingest, reconcile, evaluate and both overlay pushes.

Three phases, and the ordering is load-bearing: copy the newest 250 reports out under `intel_state`
and drop it, take `pilots` for the resolved and uncertain maps and drop it, then enrich and hash with
nothing held. The documented `intel_state -> pilots` order is satisfied because the two are never
held together at all.

**`web::facts::UiFacts`.** Pushed down once a frame from `drain_alerts`, which is already the place
that pushes UI state into `AlertEngine::config`. Kept as its own struct rather than growing
`AlertConfig`, so an optional feature never sits on the alert path.

## What the tests caught that the design did not

`CardChars` is empty for a single character, by design: the ring exists only to say *whose* number a
card is quoting, so with nobody to confuse it there is nothing to disambiguate and the card draws the
plain number. The first version of `a_card_carries_the_distance_and_ring_the_report_does_not`
asserted a hop and failed. The ticket was right about what the pane carries and I was wrong about
what it carries it in. Both cases are now covered, the empty single-character ring and a real
two-character ring, which is better coverage than the assertion I started with.

`intel_beyond_the_gates` sits in Jita, which the fixture graph connects to nothing, so its distance is
`None`. That turned into `an_unreachable_system_has_no_distance`, which is worth keeping: an
unreachable system reported as `Some(0)` would put a hostile on top of the player.

## How the tests were proven to have teeth

| Reverted | Result |
|---|---|
| the early return in `WebState::changed` | `an_unchanged_tick_publishes_nothing` and `a_new_report_moves_only_the_panes_it_touches` both fail |
| took `intel_state` again around the `pilots` lock in `tick` | `the_publisher_never_holds_two_locks_at_once` hangs, killed at 60s |

The deadlock test fails by hanging rather than by asserting, which is a worse failure mode to meet in
CI than a red assertion. It is kept anyway: a lock-order regression has no other cheap signal, and a
job that stops is at least a job someone looks at. Anyone shortening the 150ms hold in the opposing
thread should know they are tuning a race, not a timeout.

## Note on the `app.rs` budget

The series rule allows WEB-002 the `UiFacts` field and one `publish_ui_facts()` call. This also made
three existing free functions `pub(crate)` (`severity_of`, `build_last_ship`, `uncertain_set`) so the
publisher can reuse them instead of copying their bodies, added an `alerts_enabled()` accessor next to
the engine method it serves, and holds a `web` field for WEB-003 to read. Visibility and one accessor,
no logic, but it is more than the ticket said and it is recorded here rather than left to be noticed.
