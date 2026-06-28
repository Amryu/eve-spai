# CLAUDE.md

Working notes for this repository. Read before making changes.

## What this is

EVE Spai is a desktop intel and situational-awareness tool for EVE Online, written in
Rust with egui/eframe. It watches EVE chat logs, parses intel into cards, shows a star
map, raises configurable alerts, and embeds XMPP fleet chat plus zKillboard lookups.

It is an independent, ground-up project that uses only EVE's public static data. Do not
reference, compare to, or attribute it to other third-party tools in code, docs, or
commit messages.

## Build, test, run

- `cargo run --release` runs the app. Debug builds are slower than release, so confirm
  real performance with a release build — but don't dismiss a slowdown as "just debug":
  if release performance is also poor, it's a real regression, not a build artifact.
- `cargo test` runs the unit tests (intel parsing is heavily covered). Important:
  `cargo test` does NOT rebuild the `eve-spai` binary. Run `cargo build` before
  relaunching the app, or you will run a stale binary and "fixes" will look like they
  did nothing.
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
