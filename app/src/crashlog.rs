//! The panic log, and the three things that used to make it useless.
//!
//! It was the only record of a crash (a release build has no console), but it was written with
//! `let _ =` on both the open and the write, so a disk-full crash left nothing behind. It also
//! amplified itself: the hook ended by calling the default hook, which prints to stderr, and when
//! stderr is a broken pipe (the app runs detached) that print panics and re-enters the hook. One
//! real incident produced 167 near-identical lines in five seconds. And it had no size cap.

use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 1024 * 1024;
/// Written once at startup so the hook can overwrite allocated blocks instead of extending the
/// file. See [`reserve`] for what that is and is not worth.
const RESERVE_BYTES: u64 = 64 * 1024;
/// A backstop against a spinning panic loop filling the disk with its own diagnosis.
const MAX_PANICS: u32 = 64;

static PANICS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

std::thread_local! {
    /// Same-thread re-entry is the cascade; a genuine simultaneous panic on another thread is
    /// still worth recording, so this is thread-local rather than global.
    static IN_HOOK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone, Copy)]
pub(crate) enum Role {
    Main,
    Overlay,
}

impl Role {
    fn file_name(self) -> &'static str {
        match self {
            // Both processes run `main` and installed the same hook, so they appended to one file
            // and raced each other's rotation.
            Role::Main => "crash.log",
            Role::Overlay => "crash-overlay.log",
        }
    }
}

pub(crate) fn path_for(role: Role) -> Option<PathBuf> {
    Some(crate::store::data_dir().ok()?.join(role.file_name()))
}

pub(crate) fn install(role: Role) {
    if let Some(p) = path_for(role) {
        reserve(&p);
    }
    std::panic::set_hook(Box::new(move |info| {
        if IN_HOOK.with(|f| f.replace(true)) {
            return;
        }
        let n = PANICS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < MAX_PANICS {
            let loc = info.location().map(|l| l.to_string()).unwrap_or_default();
            let msg = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let thread = std::thread::current().name().unwrap_or("unnamed").to_owned();
            // Free space and pressure level go in the line so the next report answers "was the
            // disk full?" without anyone having to guess. Both are lock-free atomics, which is
            // the only reason they are safe to read from a panic hook.
            let free = crate::disk::available().map_or_else(|| "?".to_owned(), |b| b.to_string());
            let line = format!(
                "{} v{} [{thread}] {loc}: {msg} free={free} level={:?}",
                chrono::Utc::now().to_rfc3339(),
                env!("CARGO_PKG_VERSION"),
                crate::disk::level(),
            );
            if let Some(p) = path_for(role) {
                let _ = record_at(&p, &line);
            }
            // Never the default hook: it prints to stderr, and on a broken pipe that print panics
            // and lands us back here. Ours returns Err instead.
            let _ = writeln!(std::io::stderr().lock(), "{line}");
        }
        IN_HOOK.with(|f| f.set(false));
    }));
}

/// Keep a block of the file allocated so the hook writes into space the filesystem already gave
/// us. Worth being precise about what this buys: on ext4, xfs and NTFS overwriting allocated
/// blocks needs no new allocation, so the log still works at zero bytes free. On btrfs and ZFS it
/// is copy-on-write and can still fail. The real defence against a blackout is the degraded mode
/// in `disk.rs` keeping the disk off zero in the first place.
pub(crate) fn reserve(path: &Path) {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if len >= RESERVE_BYTES {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(&vec![b'\n'; (RESERVE_BYTES - len) as usize]);
    }
}

/// Appends `line`, reporting failure rather than swallowing it. Writes into the reserved tail
/// where one is left, so a full disk does not silence the black box.
pub(crate) fn record_at(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    rotate_if_needed(path);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    let end = f.seek(SeekFrom::End(0))?;
    let reserved = trailing_reserve(&mut f, end);
    if reserved > line.len() as u64 {
        f.seek(SeekFrom::Start(end - reserved))?;
    } else {
        f.seek(SeekFrom::End(0))?;
    }
    let r = f.write_all(line.as_bytes()).and_then(|()| f.write_all(b"\n"));
    if let Err(e) = &r {
        crate::disk::note_io_error(e);
    }
    r
}

/// How much of the tail is still the filler written by [`reserve`].
///
/// Reads only the last [`RESERVE_BYTES`], never the whole file: a character device such as
/// `/dev/full` reports length 0 and then yields zeros forever, so slurping the path here ran the
/// process out of memory instead of reporting the failure it was written to catch.
fn trailing_reserve(f: &mut std::fs::File, end: u64) -> u64 {
    use std::io::Read;
    let window = end.min(RESERVE_BYTES);
    if window == 0 {
        return 0;
    }
    if f.seek(SeekFrom::Start(end - window)).is_err() {
        return 0;
    }
    let mut buf = vec![0u8; window as usize];
    if f.read_exact(&mut buf).is_err() {
        return 0;
    }
    buf.iter().rev().take_while(|b| **b == b'\n').count() as u64
}

pub(crate) fn rotate_if_needed(path: &Path) {
    let over = std::fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    if !over {
        return;
    }
    let rotated = rotated_path(path);
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(path, &rotated);
}

fn rotated_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".1");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join("eve-spai-crashlog-tests");
        std::fs::create_dir_all(&d).expect("scratch dir");
        let p = d.join(name);
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(rotated_path(&p));
        p
    }

    #[test]
    fn a_line_is_recorded_and_readable() {
        let p = scratch("plain.log");
        record_at(&p, "first").expect("write");
        record_at(&p, "second").expect("write");
        let body = std::fs::read_to_string(&p).expect("read");
        assert!(body.contains("first") && body.contains("second"), "{body:?}");
    }

    #[test]
    fn rotation_caps_the_file() {
        let p = scratch("rotate.log");
        std::fs::write(&p, vec![b'x'; (MAX_BYTES + 1) as usize]).expect("seed");
        record_at(&p, "after").expect("write");
        assert!(rotated_path(&p).exists(), "the oversized log was not rotated aside");
        assert!(std::fs::metadata(&p).expect("meta").len() < MAX_BYTES);
    }

    /// The reserved tail is the point: a line has to land inside it rather than extend the file.
    #[test]
    fn a_reserved_log_does_not_grow_when_written_to() {
        let p = scratch("reserved.log");
        reserve(&p);
        let before = std::fs::metadata(&p).expect("meta").len();
        assert_eq!(before, RESERVE_BYTES);
        record_at(&p, "panic went here").expect("write");
        let after = std::fs::metadata(&p).expect("meta").len();
        assert_eq!(after, before, "the write extended the file instead of using the reserve");
        assert!(std::fs::read_to_string(&p).expect("read").contains("panic went here"));
    }

    /// The failure this whole module exists for. `/dev/full` takes writes and fails them with
    /// ENOSPC, so the real errno path is reachable without a loop device or root.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_full_disk_reports_instead_of_panicking() {
        let _guard = crate::disk::test_guard();
        let before = crate::disk::level();
        let err = record_at(Path::new("/dev/full"), "a panic on a full disk")
            .expect_err("/dev/full must refuse the write");
        // The write feeds the process-global pressure state, which every other test in this
        // binary shares. Put it back, or the UI scenes render a banner they never asked for.
        crate::disk::force_level(before);
        assert!(
            crate::disk::is_full_io(&err),
            "unrecognised: {err:?} raw={:?}",
            err.raw_os_error()
        );
    }
}
