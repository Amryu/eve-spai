# UI-038 review cycle

**Status:** Fixed and verified
**Branch:** `feat/ui-038-disk-space-hardening`

## Resolution

| | |
|---|---|
| **Outcome** | Fixed |
| **Agent time** | none for the fix, done inline; 3 explore agents and 1 plan agent for the design |
| **Patches rejected on review** | 0 |
| **App code changed** | 1177/72 lines: 596 across 8 existing files, plus `disk.rs` and `crashlog.rs` at 581 new |
| **Harness code changed** | 144/0 lines: 4 scenes, 2 assertions, 1 shot test |
| **Tests** | 538 to 555 passing, 7 to 8 ignored |
| **Archive tables with retention** | 3 of 9 to 4 of 9, and all 9 gated under pressure |
| **Battle history read** | every row ever stored to a 30-day window |
| **Follow-ups** | GAP-011 and five other gaps, below |

## What changed

`app/src/disk.rs` holds a three-level pressure state (Normal, Low, Critical) with absolute byte
thresholds, hysteresis, and a monitor thread. `app/src/crashlog.rs` takes over the panic hook.
Between them, `Store::exec_historic` / `exec_essential` put the whole historic/essential split in
one place instead of at thirty call sites.

**The guarantee.** At Critical the archive stops: `engagements`, `chats`, `pings`, `kill_intel`,
`kill_details`, `known_pilots`, `pilot_activity`, the image cache and the lookup cache. Settings,
tokens, wormholes and the user's own battle edits are always attempted. The alert daemon evaluates
in memory and its only writes are restore-caches, so alerts, sounds, the overlay and the intel feed
keep working with the archive fully off, without special-casing.

**Thresholds.** Low under 1GiB, clearing at 2GiB; Critical under 256MiB, clearing at 512MiB, and
de-escalation additionally needs three consecutive polls. Absolute bytes because a percentage is
meaningless on a 2TB disk, and measured at `store::data_dir()` because `/` on an ostree host is a
read-only image that permanently reads 100% full.

**A failed write beats a poll.** `note_sqlite_error` / `note_io_error` escalate immediately on
`SQLITE_FULL` or errno 28; only the monitor de-escalates, and only after its three readings. That
asymmetry makes an observed failure sticky without letting one wedge the app in degraded mode.

**Its own thread, not `procstat`.** `procstat::tick` runs from `status_bar`, i.e. only when a frame
renders, and the window minimises to tray while the kill firehose keeps writing. A frame-driven
poll is blind in exactly the scenario that fills the disk.

**Retention.** `ENGAGEMENT_RETENTION_SECS` is 30 days, `prune_engagements` is no longer dead code
and deletes in 20k batches so the first pass after upgrade does not hold the write lock past every
other connection's 5s timeout. `load_battle_history` now reads the same window rather than every
row ever stored. VACUUM is guarded by `should_vacuum`, which requires Normal pressure, something
actually deleted, room for a second copy of the database plus 512MiB, and a week since the last.

**The crash logger.** Re-entrancy guard, a 64-panic global cap, 1MiB rotation, per-role filename so
the two processes stop sharing a file, free space and pressure level recorded in each line, and the
default hook is no longer called at all. That last one is the actual fix for the 167-line cascade:
the default hook prints to stderr and panics on a broken pipe, so replacing it with a `writeln!`
that returns `Err` removes the amplifier rather than papering over it.

## Numbers corrected during the work

The design brief said 98% of `engagements` rows were prunable. That figure was against the **24h
in-memory TTL**, not the agreed 30-day retention. Measured: 133,579 of 256,378 rows, **52%**.
Retention reclaims roughly 187MB and settles at ~190MB total. It caps growth; it does not make the
database small, and nobody should expect a 400MB file to become a 40MB one.

## Three bugs the tests found in my own work

**An infinite read.** `crashlog::record_at` located its reserved tail with `std::fs::read(path)`.
`/dev/full` reports length 0 and then yields zeros forever, so the very test written to prove the
logger survives a full disk instead ran the test binary out of memory and died on SIGKILL. Now it
seeks and reads a bounded 64KiB window.

**A comment that was not true.** The height fix carried a claim that `Button::image` was capping
something; reverting only that constructor left the test green, so the claim was wrong and the
comment now says only what is verified. (Carried over from UI-037's follow-up round, listed here
because the same teeth-check discipline caught it.)

**Test-order flakiness, twice.** The pressure state is process-global and `cargo test` shares the
process, so a `/dev/full` test escalated the level and the layout checker then rendered scenes with
a banner nobody asked for. Fixed twice over: the banner reads a per-frame snapshot on `SpaiApp`
rather than the global, so rendering is a pure function of the app and a scene sets what it draws;
and the tests that force or observe the global take a shared `disk::test_guard()`. Four consecutive
full runs are green.

## Teeth

- `a_full_database_stops_the_archive_until_there_is_room` fills a `max_page_count`-capped database,
  then checks the app's own `save_engagement` raises the level on `SQLITE_FULL`, that a later write
  is **not attempted** even after the cap is lifted, and that it resumes once the level clears.
  `PRAGMA max_page_count` returns the same error code a real ENOSPC does.
- `classify` is pinned against flapping by walking a reading either side of a trip four times.
- `crashlog` tests: `/dev/full` returns `Err` rather than panicking; a reserved log does not grow
  when written to; an oversized log rotates.
- `uitest_disk_banner_does_not_displace_the_chrome` fails if the banner pushes the status bar out
  of an 800px window.
- **One test's teeth were disproved and it was renamed.** `a_failed_commit_leaves_the_connection_usable`
  claimed to catch the wedged-connection bug. Restoring the old hand-rolled `BEGIN`/discarded
  `COMMIT` left it green, because `max_page_count` fails the INSERT rather than the COMMIT, so the
  commit has nothing to flush. It is now
  `an_upsert_against_a_full_database_leaves_the_connection_usable`, which is what it actually
  proves, and the commit-failure case is GAP-011.

## Screenshots

`before/`: a machine with 190MB free and nothing on screen about it. `after/disk_banner_critical.png`:
an amber header, the reading, a sentence saying recording stopped and that alerts still work, an
Open data folder button and **no Dismiss**, plus a `Disk 190 MB` chip in the status bar that
survives dismissing the Low banner. The narrow render proves the wording wraps rather than clips.
The layout checker caught the first draft, where the long explanation wrapped inline beside the
headline and its bounding box covered it; it is now its own row.

## Gaps filed rather than pretended

- **GAP-011** real filesystem ENOSPC against SQLite, including a commit-time failure and a partial
  WAL frame. Needs a loop device or a privileged container.
- The panic hook end to end: a test cannot install a process-global hook and panic without racing
  every other test in the binary.
- The broken-pipe stderr cascade: needs a detached process with closed stderr, so a manual repro
  script rather than a unit test.
- Copy-on-write filesystems (btrfs, ZFS) defeating the pre-allocated crash-log region. The code
  comment says this outright rather than implying a guarantee.
- `charsettings::replicate` mid-copy truncation and `update.rs` Windows double-rename recovery.
  Both got a cheap precondition here instead; proper rollback needs platform-specific harnesses.
- VACUUM lock contention against live writers under the 5s busy timeout.

## Residual risk and stated decisions

**Data dropped while degraded is permanently lost, deliberately.** Engagements missed during an
outage are gone: the backfill window only recovers recent kills for systems on screen, so battles
during the outage are absent from history forever. Chats stay in the live session but are missing
after a restart, with nothing marking the hole. Image and lookup misses just refetch. Buffering
dropped writes in RAM was rejected: it trades a disk problem for a memory problem, on the app's
highest-volume writer.

**"Full history" is now "Last 30 days".** A real reduction in what the app offers, so the checkbox
says so and its hover text points at JSON export, which `Open JSON` already reads back.

**The mutex-poisoning sweep is deliberately out of scope, at the user's direction.** It is the
mechanism that turns one failed thread into a dead process: 372 `.lock().unwrap()` sites, 11
poison-tolerant. This change removes the disk-related triggers, so it should prevent this
recurrence, but the app remains one unrelated background panic away from the same cascade.

**Low may nag on small machines.** 1GiB free is unremarkable on a 32GB VM. Mitigated by Low being
dismissible and Critical not firing until 256MiB, where the complaint is correct. Adjust on
feedback rather than pre-optimising; a percentage rule would reintroduce the `/`-reads-full failure.
