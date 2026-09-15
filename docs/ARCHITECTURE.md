# Architecture

How EVE Spai is put together today. For build, release and working conventions see `CLAUDE.md`.

## Shape

A single Rust binary built on egui/eframe, plus two workspace crates:

| Crate | What it is |
|---|---|
| `app` | The desktop app, its alert overlay subprocess and the embedded web view. |
| `crates/br-core` | The battle-report model and clustering, shared by the app and the server. |
| `crates/server` | The battle-report sharing server (axum), its own workspace with its own lock file. |

The app runs as two processes. The main process owns the window, all state and every network
worker. It starts itself a second time with `--overlay` to draw the alert and fleet ping windows,
which have to stay on top of the game and must never take focus from it. The two talk over
length-prefixed JSON on the child's stdin and stdout (`ipc.rs`): the main process pushes a finished
alert frame (`AlertMsg`), the overlay sends clicks and verdicts back (`OverlayToMain`).

A lock file keeps one main instance per profile (`instance.rs`). A second launch asks the running
one to raise its window instead.

## Threads and data flow

The UI thread renders and applies edits. Everything that waits on disk or the network runs on its
own thread and shares results through `Arc<Mutex<..>>` state that the UI reads each frame.

| Worker | Module | Does |
|---|---|---|
| Chat log watcher | `watcher.rs` | Tails the EVE chat logs, parses each line into an intel report (`intel.rs`) and decays old reports. |
| Pilot resolver | `pilot.rs` | Resolves pilot names in reports to characters through ESI, with a persisted cache. |
| Affiliation | `affiliation.rs` | Corporation and alliance for resolved characters. |
| Activity | `activity.rs` | zKillboard and ESI checks that flag long-inactive characters as uncertain. |
| Location poller | `esi.rs` | Where each logged-in character is, and whether it is docked. |
| System status | `systemstatus.rs` | Sovereignty, incursions, faction warfare and last-hour activity per system. |
| Kill feed | `zkill.rs`, `kills.rs` | zKillboard kills near the player and the battle history built from them. |
| Wormholes | `wormholes.rs` | EVE-Scout connections merged with scanned ones. |
| Alert engine | `app/alert_engine.rs` | Matches fresh reports against alert rules, fires sounds and notifications, and feeds the overlay. |
| Jabber | `jabber.rs` | The XMPP connection: rooms, direct messages and fleet pings. |
| Web publisher | `web/publish.rs` | Builds the web view's snapshot panes from shared state. |
| Web server | `web/server.rs`, `web/sse.rs` | Serves the page, dialogs and actions, and streams snapshot changes. |
| One-shot jobs | `sde.rs`, `lookup.rs`, `charlookup.rs`, `brshare.rs`, `update.rs`, `auth.rs` | Static data download, pilot lookups, battle report sharing, update checks, SSO login. |

Outbound HTTP goes through `http::client`, and ESI name and id lookups through `universe.rs`, so
every request carries the same user agent and batches the same way.

## Storage

Everything lives under the profile directory (`store::data_dir`), which is owner-only on Unix.

- `eve-spai.db` (SQLite, `store.rs` and `store/`): EVE static data, settings as one JSON blob in the
  `kv` table, known pilots and verdicts, kill and battle history, wormholes, chats, characters and
  the notes folder tree. The schema is created with `CREATE TABLE IF NOT EXISTS`; older databases
  are upgraded in `Store::open`.
- Refresh tokens stay out of the database: the OS keychain (`tokens.rs`), or an account-bound
  encrypted file when there is none (`sealed.rs`). Only the short-lived access token is cached in SQLite.
- `image_cache/`, `lookup/`, `esi.log`, `crash.log`: caches and diagnostics, pruned under disk
  pressure (`disk.rs`).

`Settings` (`settings.rs`) is rewritten whole on every save. A field may be added with
`#[serde(default)]` but never retyped, since one field that fails to parse resets every setting.

## The desktop UI

`app.rs` holds `SpaiApp`, its construction and the frame loop. The rest of the UI is split into
`impl SpaiApp` blocks and free functions under `app/`, one area each: the intel card (`intel_card.rs`),
views (`views.rs`), the map (`map_ui.rs`, `map_panels.rs`, `map_route.rs`, `travel_ui.rs`), jabber
(`jabber_ui.rs`), battles (`battles_ui.rs`), settings and configuration windows (`settings_ui.rs`),
information windows (`info_windows.rs`), notes and tags (`notes_ui.rs`, `note_widgets.rs`), the
alert and ping windows (`alert_window.rs`), and so on. Dialogs are separate native viewports.

The left nav rail picks a view (`nav.rs`). The intel feed virtualizes rows with a per-card height
cache, because egui has no variable-height virtualization of its own.

## Notes and tags

`notes.rs` holds the folder tree (`NoteBook`), edited only through `NoteBook::apply`, and derives a
`NotesView`: the active tags and the merged tags and notes per system and pilot from online folders.
Cards, the map, the alert engine, the overlay and the web page all read the view. Edits from the app,
the overlay and the web page arrive as the same `NotesOp`.

## The web view

An opt-in local server (`web/`) mirrors the intel feed, alerts, fleet pings, jabber, the map, notes
and rescue mode to a browser on the same network, paired with a token. The page is plain ES modules
compiled into the binary (`web/assets.rs`), with no build step. The publisher keeps one snapshot per
pane and only republishes a pane whose content hash changed; the page merges pane updates from a
server-sent event stream. Page actions are posted as the same `OverlayToMain` messages the overlay
sends, and are applied by the same handler.

## Battle reports

`br-core` clusters killmails into battles and defines the report document. The app builds reports
from its kill history, exports them to files and can share them through `crates/server`, which
stores gzipped reports and serves a read-only page per report.

## Optional: capital rescue mode

The `fc-rescue` Cargo feature, off in published releases, compiles an FC-only mode that watches a
rescue channel, tracks fleet composition through ESI and checks titan range from staging
(`rescue.rs`, `app/rescue_ui.rs`). Its settings fields are never feature-gated, so a build without
the feature still round-trips a config written by one with it.

## Testing

- Unit tests sit next to the code, with the larger suites in `intel/tests.rs`, `app/tests.rs` and
  `store.rs`.
- `app/src/uitest/` renders UI surfaces headlessly with `egui_kittest` from fixtures only and checks
  layout. `webshot.sh` screenshots the web view from a fixture demo server.
- CI runs the tests on Linux with and without `fc-rescue`, and compile-checks Windows and macOS.
