//! Free-space pressure on the filesystem holding the data directory.
//!
//! The app crashed on a user's machine when the disk filled. Nothing here stops that machine from
//! filling up, so the goal is narrower: keep raising alerts on a full disk, drop only the archive,
//! and say so. See [`Kind`] for the split.
//!
//! State is process-global rather than an `Arc` threaded through constructors: ~20 places open
//! their own `Store`, and `esilog::record`, `image_cache::write_atomic`, `lookup::save_cache` and
//! `sound::ensure_tone` are free functions with no app handle to hang one off. There is one data
//! directory and one filesystem, so one answer. Every decision is a pure function taking the level
//! as an argument, so the statics are read only at the call boundary; a test that reads the live
//! statics can race one that sets them, which is why the logic below never does.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// Warn while there is still room to act.
const LOW_TRIP: u64 = 1024 * 1024 * 1024;
/// Enough headroom that a WAL checkpoint plus an in-flight image write still fit.
const CRITICAL_TRIP: u64 = 256 * 1024 * 1024;
/// Clear thresholds sit above the trips so the banner cannot flap on a single freed block.
const LOW_CLEAR: u64 = 2 * 1024 * 1024 * 1024;
const CRITICAL_CLEAR: u64 = 512 * 1024 * 1024;

/// A checkpoint or a browser cache eviction can free hundreds of megabytes and take them straight
/// back, so recovery has to hold across several polls before the archive resumes.
const CLEAR_POLLS: u8 = 3;

const POLL_NORMAL: std::time::Duration = std::time::Duration::from_secs(30);
const POLL_ELEVATED: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    Normal = 0,
    Low = 1,
    Critical = 2,
}

impl Level {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }
}

/// What a write is worth keeping on a disk that is nearly full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Settings, auth and anything the user typed. Losing it is unrecoverable, so it is attempted
    /// whatever the level says and the failure is reported instead of hidden.
    ///
    /// Constructed only by the tests that pin the matrix: production code never asks, because an
    /// essential write is simply attempted. Kept so the split is a type rather than a convention.
    #[allow(dead_code)]
    Essential,
    /// The archive: the kill firehose, chat scrollback, caches. Refetchable or expendable, and by
    /// far the highest volume, so it is what stops.
    Historic,
}

static LEVEL: AtomicU8 = AtomicU8::new(0);
static AVAILABLE: AtomicU64 = AtomicU64::new(u64::MAX);
static MEASURED_AT: AtomicU64 = AtomicU64::new(0);
/// Set when a write actually failed, which is worth saying instead of quoting a stale reading.
static SAW_FAILURE: AtomicU8 = AtomicU8::new(0);

pub(crate) fn level() -> Level {
    Level::from_u8(LEVEL.load(Ordering::Relaxed))
}

/// Free bytes as of the last poll, `None` before the first one or where it could not be read.
pub(crate) fn available() -> Option<u64> {
    match AVAILABLE.load(Ordering::Relaxed) {
        u64::MAX => None,
        v => Some(v),
    }
}

/// Unix seconds of the last successful reading, for the banner's hover text.
pub(crate) fn measured_at() -> Option<i64> {
    match MEASURED_AT.load(Ordering::Relaxed) {
        0 => None,
        v => Some(v as i64),
    }
}

/// Whether the current level was reached by an observed write failure rather than a poll.
pub(crate) fn saw_failure() -> bool {
    SAW_FAILURE.load(Ordering::Relaxed) != 0
}

pub(crate) fn writes_allowed(kind: Kind) -> bool {
    allowed(level(), kind)
}

pub(crate) fn allowed(level: Level, kind: Kind) -> bool {
    match kind {
        Kind::Essential => true,
        // Not gated at `Low`: at a gigabyte free, losing the archive costs more than the few
        // megabytes a day it saves.
        Kind::Historic => level < Level::Critical,
    }
}

/// The level `available` implies, given where we already are. The clear thresholds are only
/// consulted when the level is already elevated, which is what makes this hysteretic.
pub(crate) fn classify(available: u64, current: Level) -> Level {
    match current {
        Level::Normal => {
            if available < CRITICAL_TRIP {
                Level::Critical
            } else if available < LOW_TRIP {
                Level::Low
            } else {
                Level::Normal
            }
        }
        Level::Low => {
            if available < CRITICAL_TRIP {
                Level::Critical
            } else if available >= LOW_CLEAR {
                Level::Normal
            } else {
                Level::Low
            }
        }
        Level::Critical => {
            if available >= LOW_CLEAR {
                Level::Normal
            } else if available >= CRITICAL_CLEAR {
                Level::Low
            } else {
                Level::Critical
            }
        }
    }
}

pub(crate) fn is_full_sqlite(e: &rusqlite::Error) -> bool {
    match e {
        rusqlite::Error::SqliteFailure(f, _) => matches!(
            f.code,
            rusqlite::ErrorCode::DiskFull | rusqlite::ErrorCode::SystemIoFailure
        ),
        _ => false,
    }
}

pub(crate) fn is_full_io(e: &std::io::Error) -> bool {
    // `ErrorKind::StorageFull` is not stable on every toolchain this builds with, so match the
    // errno directly. 28 is ENOSPC on every unix; Windows reports ERROR_DISK_FULL as 112.
    #[cfg(unix)]
    let raw_full = e.raw_os_error() == Some(28);
    #[cfg(windows)]
    let raw_full = e.raw_os_error() == Some(112);
    #[cfg(not(any(unix, windows)))]
    let raw_full = false;
    raw_full
}

/// A write that just failed is more authoritative than a poll that has not fired yet, so this
/// escalates immediately. Only the monitor de-escalates, and only after [`CLEAR_POLLS`] readings,
/// which keeps one observed failure sticky without letting it wedge the app in degraded mode.
pub(crate) fn note_sqlite_error(e: &rusqlite::Error) {
    if is_full_sqlite(e) {
        escalate();
    }
}

pub(crate) fn note_io_error(e: &std::io::Error) {
    if is_full_io(e) {
        escalate();
    }
}

fn escalate() {
    LEVEL.fetch_max(Level::Critical as u8, Ordering::Relaxed);
    SAW_FAILURE.store(1, Ordering::Relaxed);
}

/// Free space where the app actually writes. Never `/`: on an ostree host that is a small
/// read-only image which permanently reads 100% full.
fn probe() -> Option<u64> {
    let dir = crate::store::data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    // A path that does not exist yet has no stats, so fall back to the nearest ancestor that does
    // rather than reporting a problem the user does not have.
    let mut probe: &std::path::Path = &dir;
    loop {
        if probe.exists() {
            return fs4::available_space(probe).ok();
        }
        probe = probe.parent()?;
    }
}

pub(crate) fn spawn_monitor(ctx: egui::Context) {
    std::thread::Builder::new()
        .name("disk-monitor".into())
        .spawn(move || {
            let mut clear_streak: u8 = 0;
            let mut first = true;
            loop {
                if !first {
                    std::thread::sleep(if level() == Level::Normal {
                        POLL_NORMAL
                    } else {
                        POLL_ELEVATED
                    });
                }
                first = false;

                // A failed syscall must not degrade the app: no reading means no opinion.
                let Some(free) = probe() else { continue };
                AVAILABLE.store(free, Ordering::Relaxed);
                MEASURED_AT.store(chrono::Utc::now().timestamp() as u64, Ordering::Relaxed);

                let current = level();
                let want = classify(free, current);
                let next = if want < current {
                    clear_streak = clear_streak.saturating_add(1);
                    if clear_streak >= CLEAR_POLLS { want } else { current }
                } else {
                    clear_streak = 0;
                    want
                };

                if next != current {
                    LEVEL.store(next as u8, Ordering::Relaxed);
                    if next == Level::Normal {
                        SAW_FAILURE.store(0, Ordering::Relaxed);
                    }
                    ctx.request_repaint();
                }
                if next != Level::Normal || current != Level::Normal {
                    crate::store::run_maintenance(next);
                }
            }
        })
        .ok();
}

/// The level is process-global and `cargo test` shares the process, so any test that forces or
/// observes it must hold this first or another test's `force_level` lands mid-assertion.
#[cfg(test)]
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
pub(crate) fn force_level(l: Level) {
    LEVEL.store(l as u8, Ordering::Relaxed);
    SAW_FAILURE.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    #[test]
    fn thresholds_are_absolute_bytes_not_a_share_of_the_disk() {
        assert_eq!(classify(50 * GB, Level::Normal), Level::Normal);
        assert_eq!(classify(2 * GB, Level::Normal), Level::Normal);
        assert_eq!(classify(900 * MB, Level::Normal), Level::Low);
        assert_eq!(classify(200 * MB, Level::Normal), Level::Critical);
        // Exactly on a trip is not yet over it.
        assert_eq!(classify(LOW_TRIP, Level::Normal), Level::Normal);
        assert_eq!(classify(CRITICAL_TRIP, Level::Normal), Level::Low);
    }

    #[test]
    fn recovery_needs_more_room_than_the_trip_did() {
        // Still Low between the trip and the clear: this gap is the hysteresis.
        assert_eq!(classify(1500 * MB, Level::Low), Level::Low);
        assert_eq!(classify(2 * GB, Level::Low), Level::Normal);
        assert_eq!(classify(400 * MB, Level::Critical), Level::Critical);
        assert_eq!(classify(600 * MB, Level::Critical), Level::Low);
        assert_eq!(classify(3 * GB, Level::Critical), Level::Normal, "a big free jumps two levels");
    }

    /// A reading that wanders either side of a trip must not move the level back and forth, or the
    /// banner appears and disappears while the user reads it.
    #[test]
    fn a_reading_hovering_at_the_trip_does_not_flap() {
        let mut level = Level::Normal;
        for free in [1020 * MB, 1030 * MB, 1010 * MB, 1040 * MB] {
            level = classify(free, level);
            assert_eq!(level, Level::Low, "{free} bytes moved the level off Low");
        }
    }

    #[test]
    fn only_the_archive_stops() {
        for level in [Level::Normal, Level::Low, Level::Critical] {
            assert!(allowed(level, Kind::Essential), "{level:?} refused an essential write");
        }
        assert!(allowed(Level::Normal, Kind::Historic));
        assert!(allowed(Level::Low, Kind::Historic), "a gigabyte free is not worth losing history");
        assert!(!allowed(Level::Critical, Kind::Historic));
    }

    #[test]
    fn a_full_disk_is_told_apart_from_every_other_error() {
        let full = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
            None,
        );
        assert!(is_full_sqlite(&full));

        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        assert!(!is_full_sqlite(&busy), "a lock timeout must not stop the archive");
        assert!(!is_full_sqlite(&rusqlite::Error::QueryReturnedNoRows));

        assert!(is_full_io(&std::io::Error::from_raw_os_error(28)));
        assert!(!is_full_io(&std::io::Error::from_raw_os_error(13)), "EACCES is not a full disk");
        assert!(!is_full_io(&std::io::Error::other("unrelated")));
    }

    /// `/dev/full` accepts writes and fails them with ENOSPC, so the real errno path is reachable
    /// without a loop device or root.
    #[cfg(target_os = "linux")]
    #[test]
    fn dev_full_is_recognised_as_a_full_disk() {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().write(true).open("/dev/full").expect("open");
        let err = f.write_all(&[0u8; 4096]).expect_err("/dev/full must refuse a write");
        assert!(is_full_io(&err), "unrecognised: {err:?} raw={:?}", err.raw_os_error());
    }

    /// No reading means no opinion. A machine where `statvfs` misbehaves must not have its archive
    /// switched off.
    #[test]
    fn an_unreadable_path_yields_no_reading() {
        assert!(fs4::available_space("/nonexistent-xyzzy/nope").is_err());
        let dir = std::env::temp_dir();
        let free = fs4::available_space(&dir).expect("temp dir has stats");
        assert!(free > 0, "temp dir reported {free} bytes free");
    }
}
