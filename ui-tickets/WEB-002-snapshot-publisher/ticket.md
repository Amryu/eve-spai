# WEB-002 &mdash; Nothing can read the app's live state from outside the UI thread

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `web/{mod,snapshot,facts}.rs`, `AlertEngine::push_overlay_update` |
| **Reported by** | user, remote web view |

## Gap

A rendered intel card needs more than an `IntelReport`: it needs jump distance from the player, the
bridge verdict, the character ring, resolved pilot ids, the uncertain set, kill info and affiliations.
Today the only assembled form of that is `ipc::AlertMsg` (`app/src/ipc.rs:28`), built by
`AlertEngine::push_overlay_update` (`app.rs:17313`) for the overlay subprocess, capped at the alert
feed and gated on a rule having `custom_window` set. There is no equivalent for the full intel feed,
for pings or for the map, and nothing a web server thread could read.

## Deliverable

`web::Snapshot` with one `Option` per pane (`intel`, `alerts`, `pings`, `map`, `meta`) plus `seq` and
a per-process `gen`. `None` is the dirty flag; each pane carries a `rev` bumped only when its input
hash changes, which is the `alert_sent_hash` pattern already at `app.rs:17458`. The alert pane embeds
`ipc::AlertMsg` verbatim rather than growing a parallel type.

`web::spawn_publisher`, a dedicated thread on a 500ms tick. `UiFacts` (`Arc<Systems>`, player
location, roster, severity rules, ttl, compact, theme, sounds) is written once per frame by a new
`SpaiApp::publish_ui_facts` at the end of `App::update`, the same way `AlertEngine::config` already
carries UI state down to the alert daemon.

`push_overlay_update`'s enrichment body is factored into `AlertEngine::build_alert_msg` so the overlay
push and the publisher share one implementation.

## Notes

**Not the egui update loop.** The whole point is that the phone keeps working while the desktop window
is minimised, and egui parks when it is.

**Not the alert daemon.** `spawn_alert_daemon` (`app.rs:18089`) runs at 400ms and already carries
`ingest_kills`, `reconcile`, `evaluate` and both overlay pushes. An optional feature does not belong
on the alert critical path.

**Locks, three phases, never nested.** Copy the newest 250 reports out under `intel_state` and drop
it. Take `pilots` for `display_ids` and `uncertain_set` and drop it. Enrich and serialize with nothing
held. The documented `intel_state -> pilots` order is then satisfied trivially. Everything needed is
already callable off the UI thread: `jumps_from_you`, `jump_via`, `build_char_rings` are free
functions or take `&Systems`, which is why `push_overlay_update` can do this from the daemon today.

Serialize once per tick into an `Arc<str>`. Per-client serialization makes 8 clients cost 8x.

## How to verify

- A test building a `Snapshot` from `uitest::fixtures` and asserting every card carries its jump
  count, via, char ring and resolved pilots.
- A test that a pane `rev` does not move when the inputs are unchanged, and does move on a real change.
  This is the test that stops the publisher pushing every tick.
- A deadlock test: hold `pilots` on a second thread for 200ms while the publisher runs, and assert the
  publisher completes. A fix that takes both locks together passes the other tests and fails this one.
- A fix would be WRONG if it serialized while holding `intel_state`, or if it grew `ipc::AlertMsg` with
  web-only fields; `ipc.rs` has four compatibility tests pinning that wire.
