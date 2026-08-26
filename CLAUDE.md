# CLAUDE.md

Working notes for this repository. Read before making changes.

## What this is

EVE Spai is a desktop intel and situational-awareness tool for EVE Online, written in
Rust with egui/eframe. It watches EVE chat logs, parses intel into cards, shows a star
map, raises configurable alerts, and embeds XMPP fleet chat plus zKillboard lookups.

It uses only EVE's public static data.

## Build, test, run

- `cargo run --release` runs the app. Debug builds are slower than release, so confirm
  real performance with a release build — but don't dismiss a slowdown as "just debug":
  if release performance is also poor, it's a real regression, not a build artifact.
- `cargo test` runs the unit tests (intel parsing is heavily covered). Important:
  `cargo test` does NOT rebuild the `eve-spai` binary. Run `cargo build` before
  relaunching the app, or you will run a stale binary and "fixes" will look like they
  did nothing.
- **`fc-rescue` is an opt-in Cargo feature, off by default.** It gates the whole FC-only
  delve911 capital-rescue mode (`app/src/rescue.rs`, the rescue window, the ESI fleet
  poller, the delve911 sound). Published releases are built WITHOUT it; build your own
  with `cargo build --release --features fc-rescue`. A bare `cargo test` therefore skips
  its tests, so use `cargo test --features fc-rescue` when touching that code, and note
  that CI's `cross-check` passes `--all-features` so the gated code keeps compiling.
  The `Settings` rescue fields are deliberately NOT gated: settings are rewritten
  wholesale on save, so a feature-off build must still round-trip a feature-on config
  instead of silently dropping every `rescue_*` key.
- The version lives once in the root `Cargo.toml` `[workspace.package]`; `app` inherits
  it (user-agent strings track it via `env!("CARGO_PKG_VERSION")`).
- **Bumping a version means starting a GitHub build run.** It is NOT enough to edit the
  `Cargo.toml` version — that builds nothing. To bump a version you must: (1) edit the
  `Cargo.toml` `[workspace.package]` version, (2) commit it and push `main`, then (3)
  create and push the matching `vX.Y.Z` git tag (`git tag vX.Y.Z && git push origin
  vX.Y.Z`). Pushing the tag is the trigger for the release workflow
  (`.github/workflows/release.yml`): it cross-builds every platform and publishes the
  GitHub Release. Without the pushed tag no release is produced (this is what stalled
  releases between 0.2.10 and 0.3.6). **`Cargo.toml` is the source of truth for the
  version** — the release workflow now VERIFIES that the pushed tag matches the committed
  `[workspace.package]` version and FAILS the build on a mismatch (it no longer silently
  overwrites Cargo.toml from the tag). So `Cargo.toml` must already equal `X.Y.Z` before
  you push `vX.Y.Z`. Keep `Cargo.toml`, the tag, and the release in sync.
- **After tagging a release, bump `Cargo.toml` to the NEXT version.** A LOCAL `cargo
  build` uses the `[workspace.package]` version verbatim. If `Cargo.toml` is left at the
  just-released version (or older), every local/dev build reports that stale version and
  the in-app update check flags it as "a version behind" against the published release. So
  the moment a `vX.Y.Z` release is cut, bump `Cargo.toml` to `X.Y.(Z+1)` for ongoing dev.
  This is what stranded 0.3.6: v0.3.7 was tagged/released but `Cargo.toml` stayed 0.3.6.
  CI enforces this: `.github/workflows/version-check.yml` runs on every push to `main` and
  FAILS if `Cargo.toml` is behind the latest release tag.

## Release process

- A release is a git tag `vX.Y.Z` plus a GitHub Release carrying one binary per platform.
- Asset names must be exactly: `eve-spai-linux-x86_64`, `eve-spai-macos-aarch64`,
  `eve-spai-windows-x86_64.exe`. The installers match on these names; do not rename them.
- Only Linux x86_64 can be built locally. macOS (arm64) and Windows binaries need CI
  cross-builds.
- Do NOT publish a release until its binaries are built and attached. An empty release
  breaks the installers and is pulled as "latest".
- The version in Cargo.toml, the tag, and the published release should agree. If two
  releases exist, the higher one is "latest" and is what the installers fetch.

## Install process

- `install.sh` (Linux/macOS) and `install.ps1` (Windows) resolve `/releases/latest`,
  download the asset for the host platform, and place the binary in a user dir
  (`~/.local/bin` or `%LOCALAPPDATA%\Programs\eve-spai`), overridable with `PREFIX` /
  `$env:EVE_SPAI_DIR`. So the newest release must carry every platform's asset.
- `install.sh` downloads from the predictable public URL
  `https://github.com/<repo>/releases/download/<tag>/<asset>`. An earlier version tried
  to parse the asset id from the API JSON with `grep -A2` after the name, but GitHub
  emits `"id"` BEFORE `"name"`, so it always reported "no asset". Always verify the
  installers end-to-end against a real published release.
- GitHub's raw CDN (raw.githubusercontent.com) caches for a few minutes, so a freshly
  pushed install script is not served immediately; test the local file to confirm logic.

## Lessons learned

- **Case is NOT a deciding factor in parsing.** Unless explicitly stated, do not branch pilot/
  ship/system decisions on upper- vs lower-case (EVE names may be any case). You may *suggest* a
  case-based heuristic, but do not implement one. Prefer ESI/local-cache resolution and structural
  guards (e.g. a word already consumed by a longer name must not be double-consumed) instead.
- Verify the install path end-to-end against a real release; a plausible script can still
  be wrong (the asset-id bug above).
- The parser handles **plain-text chat-LOG lines only**. EVE chat logs carry NO
  `<url=...>` tags — pilots and ships arrive as plain text, so parsing cannot rely on link
  markup. The log reader strips the `[ timestamp ] Sender >` framing and passes the
  message body plus the reporter separately, so `analyze` never sees an author prefix
  either. The in-game COPY format (`<url=showinfo:...>` tags + per-message "Name >"
  prefixes) is NOT supported — that machinery was removed. Write all parser tests as plain
  text; never put `<url=>` tags or a "Name > " prefix in a test (those are just how
  in-game examples get pasted into chat, not our input).
- ESI `/universe/ids/` POST: keep batches under ~200 names (1000 -> HTTP 400, 500 ->
  504). Make a failed batch return an `Option` so it does not poison the
  not-a-character cache.
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
- Platforms: `ksni` (tray) is gated to Linux in Cargo.toml; sound, log paths, and the
  X11/xdotool window helpers are cfg'd per OS and return None/no-op off Linux. Non-Linux
  branches cannot be compile-checked here, so keep their stubs trivial.
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

- `intel_row` skips any pilot missing from `resolved_pilots` (app.rs), so unresolved fixture
  names render nothing at all.

The `uncertain` set used to be a second one, keyed by lowercase with nothing saying so. It is now
`pilot::UncertainPilots`, which lowercases on construction and matches case-insensitively, so a
fixture can pass display-cased names.

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

## UI issue workflow

Findings from the harness become tickets under `ui-tickets/`, one folder per ticket:
`ui-tickets/UI-NNN-slug/` with `ticket.md`, `before/` screenshots, and `review.md`.
`ui-tickets/README.md` is the index.

The cycle for each ticket, documented in its `review.md` every time:

1. **Ticket** states the symptom, the screenshot that shows it, the file:line cause, severity,
   and how to verify the fix.
2. **Fix** happens in a git worktree, one agent per ticket: `git worktree add --detach <path>
   main`, so the agent's `git diff HEAD` is its fix alone. If the tree carries uncommitted work,
   seed the worktree with `git diff HEAD` applied and committed as a base first.
3. **Review** the returned patch before applying it. Check it addresses the cause and not the
   symptom, touches only its own region, and adds no comment that restates the code.
4. **Verify** by applying the patch to the main tree, re-running `uitest`, re-rendering the
   scene, and looking at the new PNG against `before/`. Save it as `after/`.
5. **Record** all of the above in `review.md`: what changed, what the screenshots show, what was
   rejected and why, and the test counts before and after.

Each fix lands as its own branch, merged back with `--no-ff`:

```bash
git checkout -b fix/ui-NNN-slug main
git apply <the agent's patch>        # plus its review.md and after/ screenshots
git commit
git checkout main
git merge --no-ff fix/ui-NNN-slug
```

One merge commit carries the code, its `review.md` and its `after/` screenshots together, so a fix
reads like a PR and backs out with `git revert -m 1 <merge-commit>` without disturbing the others.
Never squash several tickets into one commit; that is the whole point of the branch.

Rules that matter:

- Quote the test count to an agent as a FLOOR, never as an exact number. Told "must be green (11
  passed)", an agent reads that as "must not change" and skips adding the regression test the
  ticket most needs. Say that adding tests is welcome and the count going up is the goal.
- Two agents paired by region of `app.rs` still collide in `app/src/uitest/scenes.rs`, because
  every ticket adds a test there. Expect `git apply -3` on the second patch of a wave, and check
  the resolve: a conflict boundary can truncate a test mid-function, which surfaces as an unclosed
  delimiter rather than as a quietly dropped assertion.
- At most 2 agents in parallel, and never two whose fixes touch the same region. Most of the UI
  lives in `app/src/app.rs`, so pair by region: `intel_row`, `render_ping`, `battles_view`,
  `settings_view`, the alert viewport callback, `nav.rs`. Cross-cutting changes run alone.
- Apply patches to the main tree one at a time and re-run the suite between them.
- A fix is not done until a screenshot shows it fixed. The exception is an interaction the harness
  cannot reach cheaply, drag-and-drop being the known one: seed the state and render the result
  rather than simulating the input, and if even that fights back, land the fix and record what is
  uncovered. Verification effort is meant to be proportionate, not total.

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
