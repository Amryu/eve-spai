# CLAUDE.md

Working notes for this repository. Read before making changes.

## What this is

EVE Spai is a desktop intel and situational-awareness tool for EVE Online, written in Rust with
egui/eframe. It watches EVE chat logs, parses intel into cards, shows a star map, raises
configurable alerts, keeps notes and tags, embeds XMPP fleet chat and zKillboard lookups, and can
mirror itself to a local web view. It uses only EVE's public static data.

`docs/ARCHITECTURE.md` describes how the pieces fit together.

## Build, test, run

- `cargo run --release` runs the app. Debug builds are slower, so confirm performance on a release
  build, but a slowdown that also shows in release is a real regression.
- `cargo test` does NOT rebuild the `eve-spai` binary. Run `cargo build` before relaunching the
  app, or you run a stale binary and a fix looks like it did nothing.
- **`fc-rescue` is an opt-in Cargo feature, off by default.** It gates the FC-only delve911
  capital-rescue mode (`rescue.rs`, `app/rescue_ui.rs`, the ESI fleet poller, the delve911 sound).
  Published releases are built without it; build your own with
  `cargo build --release --features fc-rescue`. A bare `cargo test` skips its tests, so use
  `cargo test --features fc-rescue` when touching that code. CI tests with and without it.
  The `Settings` rescue fields are deliberately not gated: settings are rewritten whole on save, so
  a feature-off build must still round-trip a feature-on config.
- The version lives once in the root `Cargo.toml` `[workspace.package]`; `app` inherits it.

## Release process

- A release is a git tag `vX.Y.Z` plus a GitHub Release carrying one binary per platform.
- To release: set `Cargo.toml` `[workspace.package]` version to `X.Y.Z`, commit and push `main`,
  then `git tag vX.Y.Z && git push origin vX.Y.Z`. The pushed tag triggers
  `.github/workflows/release.yml`, which builds every platform and publishes the release. Editing
  the version alone builds nothing.
- `Cargo.toml` is the source of truth: the release workflow fails when the tag does not match it.
- Right after a release, bump `Cargo.toml` to `X.Y.(Z+1)`. A local build reports the
  `Cargo.toml` version, so leaving it at the released one makes every dev build flag itself as a
  version behind. `version-check.yml` fails on `main` when `Cargo.toml` is behind the latest tag.
- Asset names must be exactly `eve-spai-linux-x86_64`, `eve-spai-macos-aarch64` and
  `eve-spai-windows-x86_64.exe`; the installers match on them.
- Only Linux x86_64 builds locally. macOS and Windows binaries come from CI.
- Do not publish a release before its binaries are attached: an empty release breaks the
  installers and is served as "latest".

## Install process

- `install.sh` (Linux/macOS) and `install.ps1` (Windows) resolve the latest release, download the
  asset for the host and place it in a user directory (`~/.local/bin` or
  `%LOCALAPPDATA%\Programs\eve-spai`), overridable with `PREFIX` / `$env:EVE_SPAI_DIR`.
- `install.sh` uses the predictable URL `https://github.com/<repo>/releases/download/<tag>/<asset>`.
  GitHub's API emits `"id"` before `"name"`, so parsing the asset id out of the JSON is fragile.
- Verify installer changes end-to-end against a real published release.
- raw.githubusercontent.com caches for a few minutes, so test a freshly pushed script locally.

## Lessons learned

- **Case is NOT a deciding factor in parsing.** Unless explicitly stated, do not branch pilot, ship
  or system decisions on upper- vs lower-case (EVE names may be any case). You may *suggest* a
  case-based heuristic, but do not implement one. Prefer ESI/local-cache resolution and structural
  guards (e.g. a word already consumed by a longer name must not be double-consumed).
- The parser handles **plain-text chat-log lines only**. Chat logs carry no `<url=...>` tags:
  pilots and ships arrive as plain text. The log reader strips the `[ timestamp ] Sender >` framing
  and passes the body and the reporter separately, so the parser never sees an author prefix. The
  in-game copy format (`<url=showinfo:...>` tags, per-message "Name >" prefixes) is not supported.
  Write parser tests as plain text; never put `<url=>` tags or a "Name > " prefix in a test.
- ESI `/universe/ids/` POST: keep batches under ~200 names (1000 gives HTTP 400, 500 gives 504).
  Make a failed batch return an `Option` so it does not poison the not-a-character cache.
- The persisted known-pilot cache will match real players named like common words
  ("Navy", "Comet", "Issue", "Wormhole") anywhere. Match it against the ship-masked text
  and skip stop-words. Intel keyword words ("wormhole", "cap", "tackled", ...) belong in
  the pilot stop-list so they are not double-parsed as pilots.
- Pilot recognition: prefer the longest real name, resolve 1-3 word sub-spans via ESI,
  and keep ship/keyword vocabulary out of name runs. The cover splits over-glued runs.
- egui has no built-in variable-height virtualization. The intel feed virtualizes
  manually with `show_viewport` plus a per-card height cache. Coalesce background
  repaints (e.g. the pilot resolver) to ~1 fps so the feed does not churn when only the
  clock is ticking.
- Platforms: the tray is `ksni` on Linux and `tray-icon` on Windows and macOS; sound, log paths
  and window helpers are cfg'd per OS. Windows and macOS code cannot be compiled locally; the
  `cross-check` workflow compile-checks it, so keep non-Linux branches simple.
- UI: confirm a phosphor icon exists (grep the crate) before using it, or it renders as a
  tofu square. Never use small font sizes for content text.

## Conventions

- Commits and PRs carry no AI attribution or co-author trailer. PR bodies are change
  bullets.
- Do not mutate the user's real config or database during verification; prefer unit tests
  and scratch dirs.
- Push and publish only when asked.

## Headless UI harness

`app/src/uitest/` renders any UI surface to a PNG without launching the app, and drives
hover/click through the AccessKit tree. No real profile, no network, no threads, no display
server. Built on `egui_kittest`, version-locked to egui.

- `cargo test --bin eve-spai uitest` runs layout and interaction assertions. No GPU, ~1.5s.
- `cargo test --bin eve-spai uitest_screenshots -- --ignored` writes PNGs to `target/uishots/`.
  Each scene renders twice: plain, and `.debug.png` with egui's interactive-widget overlay.
- `cargo test --bin eve-spai uitest_census -- --ignored --nocapture` prints per-scene hit-target
  counts, the smallest target, and a role histogram.

`checks.rs` catches overlapping click targets, overlapping text, horizontally escaped widgets,
zero-area hit rects, and content wider than its window. It is blind to painted decoration
(separators, canvas art) because those emit no AccessKit node, so the screenshots stay the
primary signal and the assertions are the regression gate.

One trap that silently guts a fixture, hit once already:

- `intel_row` skips any pilot missing from `resolved_pilots` (`app/intel_card.rs`), so unresolved
  fixture names render nothing at all.

`pilot::UncertainPilots` lowercases on construction and matches case-insensitively, so a fixture can
pass display-cased names.

Screenshot renders are deterministic. For a refactor that should not change the UI, render
`uitest_screenshots` before and after and compare the PNGs pixel for pixel.

Add a scene by appending to `scenes::all()`. Check the census afterwards: a scene near the
~12-target chrome baseline is not being inspected in any meaningful sense.

Size a scene to its whole subject. A scene that crops what it is meant to show is worse than no
scene, because it reads as coverage. Two tickets in the first round attached `before/` screenshots
that did not contain the bug: UI-009's chip never rendered in the scene at all, and UI-011's footer
sat 350px below the frame. Both were caught by the agent doing the fix, not by the review.

`SpaiApp::build(ctx, headless: true)` skips the image loaders, the control socket, all
background threads, the tray and the overlay subprocess, and refuses to open a store unless
`EVE_SPAI_DATA_DIR` is set. Headless also disables the workers that populate views, so
async-populated views show permanent loading states.

**Never render real data. Every scene uses fixtures, never the live profile.** Screenshots get
committed to `ui-tickets/*/before|after/` and pushed to a public repo, and alliance chat is
operational information: room names, contact JIDs, fleet pings and intel must not leave the
machine in a PNG. `harness::build` and `harness::shot` both call `assert_no_live_profile`, which
fails the test unless `EVE_SPAI_DATA_DIR` (and `store::data_dir()` through it) resolves to
`target/uitest-profile`. Do not weaken that guard to "just get a real-looking screenshot", and do
not paste real chat into a fixture; write plausible fake traffic instead. If a render needs data
the fixtures do not have, add it to `fixtures.rs`.

## Web view screenshots

The web view (`app/src/web/`) is HTML in a browser, so the egui harness cannot render it and
`checks.rs` cannot see it. `GAP-011` records what that leaves uncovered. What stands in for it:

- `app/src/uitest/webshot.sh` shoots the fixture demo at 1440 and 390 into `target/webshots`.
- It always restarts `cargo test --bin eve-spai webdemo -- --ignored --nocapture` on port 6799,
  which serves `uitest::fixtures` on loopback, so a changed asset is never served stale. **Never the live profile**: the same rule as the egui
  renders, for the same reason.

Three traps, each of which silently produces nothing rather than an error:

- The flatpak Firefox can only write under `xdg-download`. A `--screenshot /tmp/x.png` succeeds and
  writes no file, and `--profile` outside that path reports "Could not find profile folder".
- Without `--no-remote` and its own profile, a running Firefox swallows the URL and exits 0.
- `--screenshot` fires on the `load` event, so anything the page fetches afterwards is not in the
  shot. The page inlines its first snapshot as a JSON island for that reason, which is also one less
  round trip on a phone.

To stop the demo, kill it **by port** (`fuser -k 6799/tcp`). A `pkill -f` broad enough to match the
test binary also matches the shell that ran it, which kills the session (exit 144).

## UI issue workflow

UI defects go through the `ui-tickets` skill: `.claude/skills/ui-tickets/SKILL.md`. Read it before
filing, fixing, reviewing or landing one, and before dispatching a fix agent.

The shape, so it is recognisable without loading the skill: one folder per ticket under
`ui-tickets/UI-NNN-slug/` holding `ticket.md`, `before/`, `after/` and `review.md`; `GAP-NNN` for
what the harness cannot reach; at most two agents at once and never in the same region; one branch
per ticket merged `--no-ff` so `git revert -m 1` backs the whole thing out.

The skill carries the ticket and review templates, the agent brief, the harness traps and the
incident log behind every rule. It is meant to grow: when a round teaches something, add the rule
and the incident in the same commit as the work that taught it.

## Writing and comments (stop slop)

Applies to prose (replies, PR bodies, commit messages) and to code comments. Adapted from the
"stop slop" skill (github.com/hardikpandya/stop-slop).

- NEVER use em-dashes. Use commas for a pause. Do not overuse hyphens or semicolons; prefer plain
  commas and periods.
- Cut filler: no throat-clearing openers ("here's what", "in general"), no emphasis adverbs, no
  softening or hand-holding. State facts directly, in active voice, with a human subject.
- Avoid formulaic shapes: "not X, it's Y" contrasts, negative listings, rhetorical Wh- setups,
  three sentences of equal length in a row, paragraphs that end on a punchy one-liner.
- Be specific. Drop lazy extremes ("always", "never", "every") unless they are literally true.
- Comments are terse and rare. Write one only when a piece of code needs a specific justification
  (WHY it is this way, e.g. a non-obvious workaround or constraint), never to restate WHAT the code
  already says. Default to no comment.
