# Architecture

How EVE Spai is put together today. For build, release and working conventions see `CLAUDE.md`.

## Shape

A single Rust binary built on egui/eframe, plus the workspace crates:

| Crate | What it is |
|---|---|
| `app` | The desktop app and the embedded web view. |
| `crates/br-core` | The battle-report model and clustering, shared by the app and the server. |
| `crates/server` | The sharing server (axum): battle reports, the wormhole map, the fitting simulator's presets and libraries; its own workspace with its own lock file. |
| `crates/spai-ui` | Widgets, theme and i18n shared by every egui front end. |
| `crates/spai-fit` | The fitting engine: the baked dogma dataset, the modifier calculation, stats, graphs, formats, variations and fleets. No egui, no I/O. |
| `crates/spai-fit-ui` | The fitting simulator's screens, shared by the desktop tab and the browser; all I/O goes through `HostRequest`/`HostEvent`. |
| `crates/spai-webkit` | What the browser apps share: page and storage helpers, EVE sign-in, ESI characters. |
| `crates/spai-web` | The wormhole map in the browser (trunk, `/wh/`). |
| `crates/spai-sim-web` | The fitting simulator in the browser (trunk, `/sim/`). |

One process owns the windows, all state and every network worker. The alert and fleet ping windows
are deferred viewports of it, kept on top of the game without taking focus from it. The alert
engine thread works out their cards (`AlertMsg`) and writes them into the window's shared state;
clicks and verdicts come back through the same state and are handled on the UI thread.

eframe skips `App::ui` while the main window is minimized or covered, and a pass that does not show
a pop-out closes it, so `App::logic` runs the same pass then, minus the main window's own panels.
Pop-outs, alerts, the assistant's voice and the timed checks keep running that way.

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
| Alert engine | `app/alert_engine.rs` | Matches fresh reports against alert rules, fires sounds and notifications, and fills the alert window. |
| Jabber | `jabber.rs` | The XMPP connection: rooms, direct messages and fleet pings. |
| Web publisher | `web/publish.rs` | Builds the web view's snapshot panes from shared state. |
| Web server | `web/server.rs`, `web/sse.rs` | Serves the page, dialogs and actions, and streams snapshot changes. |
| One-shot jobs | `sde.rs`, `lookup.rs`, `brshare.rs`, `update.rs`, `auth.rs` | Static data download, pilot lookups, battle report sharing, update checks, SSO login. |

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
Cards, the map, the alert engine, the alert window and the web page all read the view. Edits from the
app and the web page arrive as the same `NotesOp`.

## The web view

An opt-in local server (`web/`) mirrors the intel feed, alerts, fleet pings, jabber, the map, notes
and rescue mode to a browser on the same network, paired with a token. The page is plain ES modules
compiled into the binary (`web/assets.rs`), with no build step. The publisher keeps one snapshot per
pane and only republishes a pane whose content hash changed; the page merges pane updates from a
server-sent event stream. Page actions are posted as `UiAction` messages (`ipc.rs`) and applied by
the handler the app's own actions use.

## Fitting simulator

The Imperium tab (labelled Fitting for pilots outside the coalition) hosts `spai-fit-ui`. The
engine reads one blob, `dogma-v1.bin.gz`, baked from the official static data by `spai-fit-bake`:
the desktop bakes it along with its universe import and falls back to the copy the website mirrors;
the browser fetches the mirrored one. The dataset, the calculation and every formula are written
up in `docs/FITTING_MECHANICS.md`, each rule with the test that pins it. Fits and fleets live in the
desktop store and the browser's local storage, synced by revision to the server for a signed-in
character; doctrine presets come from the server, the Imperium's own and the enemy's only for
coalition members, published by the characters named in `SIM_ADMIN_CHAR_IDS`.

## Battle reports

`br-core` clusters killmails into battles and defines the report document. The app builds reports
from its kill history, exports them to files and can share them through `crates/server`, which
stores gzipped reports and serves a read-only page per report.

## Fleet command

Every build carries it, locked until the fleet dashboard confirms a commander. Two halves of one
thing: the GSF
dashboard mirror (`fleets/`, `app/fleet_ui.rs`) starts and tracks fleets, requests pings and reads
composition, boosts and doctrine. The FC-only capital rescue (`rescue.rs`, `app/rescue_ui.rs`)
watches a rescue channel, checks titan range from staging, and runs on a fleet preset tagged
Capital Save: its ping and doctrine come from that preset and its Start tracking hands over to the
fleet tab.

`fleets/backend.rs` builds every dashboard request as a `CallRecord` and performs none, so the
spoof (`spoof.rs`) and the real client (`http.rs`) are checked against one conformance suite. A
backend reports a `Mode`: `DryRun` invents its data, `ReadOnly` reads live and holds writes, `Live`
sends. `fleet-auth` adds the sign-in webview that mints the session cookie, in a subprocess because
wry needs a GTK main loop and eframe already owns the main thread with winit.

## Testing

- Unit tests sit next to the code, with the larger suites in `intel/tests.rs`, `app/tests.rs` and
  `store.rs`.
- `app/src/uitest/` renders UI surfaces headlessly with `egui_kittest` from fixtures only and checks
  layout. `webshot.sh` screenshots the web view from a fixture demo server.
- CI runs the tests on Linux, and compile-checks Windows and macOS.
