//! Rolling diagnostic log for ESI responses (docs/DESIGN.md — diagnostics).
//!
//! Pilot-name resolution sometimes returns "resolved 0/200" with no clue why. This module
//! records the EXACT ESI response (status + body) for the failing / empty batches to a
//! size-capped rolling file, so the raw bytes can be inspected after the fact instead of
//! scrolling stderr. It is best-effort: any IO error is swallowed, it never panics.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Rotate once the live log passes this size. On-disk usage is bounded to ~2x this (the live
/// file plus one rotated `esi.log.1`).
const MAX_BYTES: u64 = 10 * 1024 * 1024;

/// A single `detail` is truncated to this many bytes so one huge response body can't bloat a
/// record (or the file) without bound.
const DETAIL_CAP: usize = 8 * 1024;

/// Serialize the rotate+write so two resolver threads can't interleave a rotation with a write.
static LOCK: Mutex<()> = Mutex::new(());

/// Append a timestamped record for `context` (+ `detail`) to `<data_dir>/esi.log`, rotating the
/// file if it has grown past [`MAX_BYTES`]. Best-effort: does nothing on any IO error, never panics.
pub fn record(context: &str, detail: &str) {
    let Ok(dir) = crate::store::data_dir() else {
        return;
    };
    record_at(&dir.join("esi.log"), context, detail);
}

/// Rotate-then-write the record to `log_path` (base path taken as a parameter so tests can point
/// it at a scratch dir, honoring the no-config-mutation rule). Holds [`LOCK`] for the whole op.
fn record_at(log_path: &Path, context: &str, detail: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    rotate_if_needed(log_path);

    let ts = chrono::Utc::now().to_rfc3339();
    let mut block = String::with_capacity(DETAIL_CAP + 128);
    block.push_str(&ts);
    block.push(' ');
    block.push_str(context);
    block.push('\n');
    block.push_str(truncate_bytes(detail, DETAIL_CAP));
    block.push('\n');
    block.push('\n');

    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = f.write_all(block.as_bytes());
    }
}

/// If the live log exceeds [`MAX_BYTES`], drop `esi.log.1`, move `esi.log` -> `esi.log.1`, and
/// continue with a fresh (absent) live file. Keeps exactly one old generation.
fn rotate_if_needed(log_path: &Path) {
    let over = std::fs::metadata(log_path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    if !over {
        return;
    }
    let rotated = rotated_path(log_path);
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(log_path, &rotated);
}

/// `esi.log` -> `esi.log.1` (append `.1` to the file name).
fn rotated_path(log_path: &Path) -> PathBuf {
    let mut name = log_path.file_name().unwrap_or_default().to_os_string();
    name.push(".1");
    log_path.with_file_name(name)
}

/// Truncate `s` to at most `cap` bytes on a char boundary, appending a marker when cut.
fn truncate_bytes(s: &str, cap: usize) -> &str {
    if s.len() <= cap {
        return s;
    }
    let mut end = cap;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch dir under the OS temp dir (never the real config/data dir).
    fn scratch_dir() -> PathBuf {
        let mut p = std::env::temp_dir();
        let uniq = format!(
            "eve-spai-esilog-test-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        );
        p.push(uniq);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn rotates_after_exceeding_cap() {
        let dir = scratch_dir();
        let log = dir.join("esi.log");
        let rotated = dir.join("esi.log.1");

        // Each record carries a ~64 KiB detail (capped to 8 KiB on write). Enough calls push the
        // live file past the 10 MiB cap and trigger a rotation.
        let big = "x".repeat(64 * 1024);
        for _ in 0..2000 {
            record_at(&log, "rotation test", &big);
            if rotated.exists() {
                break;
            }
        }

        assert!(rotated.exists(), "esi.log.1 should appear once the live log passes the cap");
        let live_len = std::fs::metadata(&log).map(|m| m.len()).unwrap_or(0);
        assert!(
            live_len <= MAX_BYTES,
            "live esi.log should be small after rotation, was {live_len} bytes"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detail_is_truncated_to_cap() {
        let dir = scratch_dir();
        let log = dir.join("esi.log");
        let huge = "y".repeat(DETAIL_CAP * 4);
        record_at(&log, "cap test", &huge);
        let contents = std::fs::read_to_string(&log).unwrap();
        // Header + one capped detail + blank line: nowhere near the 32 KiB raw detail.
        assert!(contents.len() < DETAIL_CAP + 256, "detail should be capped, file was {} bytes", contents.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncate_bytes_respects_char_boundaries() {
        // A multibyte char straddling the cap must not be split (would panic on a bad slice).
        let s = "aé"; // 'a' = 1 byte, 'é' = 2 bytes
        assert_eq!(truncate_bytes(s, 2), "a");
        assert_eq!(truncate_bytes(s, 1), "a");
        assert_eq!(truncate_bytes(s, 3), "aé");
    }
}
