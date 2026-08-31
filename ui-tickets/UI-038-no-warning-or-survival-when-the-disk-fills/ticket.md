# UI-038 &mdash; A full disk kills the app, with no warning before and no record after

| | |
|---|---|
| **Severity** | Critical |
| **Status** | Open |
| **Region** | `store.rs` write paths, `main.rs` panic hook, `root_chrome` |
| **Reported by** | user, from another user's machine |

## Symptom

EVE Spai crashed when the machine ran out of disk space. Nothing warned the user beforehand, and
nothing explained it afterwards. For a tool people run while flying, that means the alerts stop at
the moment they are being relied on, with no visible cause.

The user's ask: "Both a warning and a crash prevention should be implemented. Worst case, stop
writing historic data until enough space is free again."

## Measured

| | Before |
|---|---|
| Warning surfaces for low disk space | none anywhere in the app |
| Free-space checks in the codebase | none; `fs4` was a dependency, used only for the instance lock |
| `crash.log` entries for the incident | **zero** |
| Archive tables with a retention policy | 3 of 9 (`kill_intel`, `kill_details`, `wormholes`) |
| `engagements` on the reporting profile | 360MB of a 407MB database, 256,378 rows, oldest dated 2008 |
| Rows past the 30-day window | 133,579, **52%** |
| Growth | ~96k rows / ~115MB per month |

`before/disk_banner_critical.png` is the whole of it: a machine with 190MB left, and nothing on
screen saying so.

## Cause

Three independent defects, none of which is a panic in production code. All 43 `.unwrap()`/
`.expect()` calls on I/O or rusqlite results are inside `#[cfg(test)]`; the danger is what the app
swallows instead.

**1. Nothing measures free space, and nothing degrades.** Every historic write is attempted
unconditionally, so a full disk produces a storm of failed writes that are discarded with
`let _ =` (30+ sites in `store.rs`), teaching nobody anything. The only detector,
`Store::write_probe` (`store.rs:310`), runs once at startup and `app.rs:781` reports its
`SQLITE_FULL` as "the database is not writable", with dialog text blaming file permissions.

**2. Settings can be destroyed silently.** `persist()` (`app.rs:13599`) clears `needs_save` even
when the save failed, and reports only through an `eprintln!` a release build cannot show, so saves
stop for the session with no signal. The `settings.bad` stash (`store.rs:379`) is written and read
by nothing (`grep -rn settings.bad app/src` returns one hit, the write), and the stash is itself
`let _ =`, so on a full disk it fails and the next successful save overwrites the real settings
with defaults.

**3. The black box is the first thing to fail.** `main.rs:152` opens with `if let Ok` and writes
with `let _ =`. It also amplifies itself: the hook ends by calling the default hook, which prints
to stderr, which is a broken pipe when the app runs detached, so printing the panic panics and
re-enters the hook. The real log holds 167 near-identical lines in five seconds. No rotation, no
cap, and both processes share one file.

Contributing: `zkill.rs:23` defines a 24h `ENGAGEMENT_TTL` applied only to the in-memory buffer,
while `store.rs:877` `prune_engagements` is `#[allow(dead_code)]` with zero callers, so the kill
archive grows without limit. `auto_vacuum = 0`, so deleting alone would return nothing to the
filesystem.

## Notes

- The incident was on a different machine, so its numbers are unknown. The fix must not assume this
  app is the largest consumer: another program filling the disk has to be survivable too.
- Measure the filesystem holding `store::data_dir()`, never `/`. On an ostree host `/` is a small
  read-only composefs image that permanently reads 100% full.
- Absolute byte thresholds only. A percentage is meaningless on a 2TB disk and misleading on a 32GB
  one.
- `fs4::available_space` is `#[cfg(any(unix, windows))]` and not behind a cargo feature, so it is
  usable with the existing dependency and no Cargo.toml change.
- The alert path writes nothing it needs: `spawn_alert_daemon` evaluates in memory, and its only
  writes are restore-caches. So "keep alerting on a full disk" is achievable without special-casing.

## How to verify

`cargo test --bin eve-spai disk`, `... crashlog`, `... store::tests`, `... uitest_disk_banner`, and
`cargo test --bin eve-spai uitest_screenshots_disk_banner -- --ignored` against `before/`.

The fix is wrong if it stops the app writing anything the user cannot get back (settings, tokens,
their own wormhole and battle edits), if it keeps raising a banner on a machine that is merely not
empty, if the banner displaces the nav rail or status bar, or if it buffers dropped archive writes
in memory, which trades a disk problem for a memory one on the highest-volume writer in the app.
