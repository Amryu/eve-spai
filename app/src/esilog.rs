use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BYTES: u64 = 10 * 1024 * 1024;

const DETAIL_CAP: usize = 8 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

pub fn record(context: &str, detail: &str) {
    let Ok(dir) = crate::store::data_dir() else {
        return;
    };
    record_at(&dir.join("esi.log"), context, detail);
}

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

fn rotate_if_needed(log_path: &Path) {
    let over = std::fs::metadata(log_path).map(|m| m.len() > MAX_BYTES).unwrap_or(false);
    if !over {
        return;
    }
    let rotated = rotated_path(log_path);
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(log_path, &rotated);
}

fn rotated_path(log_path: &Path) -> PathBuf {
    let mut name = log_path.file_name().unwrap_or_default().to_os_string();
    name.push(".1");
    log_path.with_file_name(name)
}

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
        assert!(contents.len() < DETAIL_CAP + 256, "detail should be capped, file was {} bytes", contents.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncate_bytes_respects_char_boundaries() {
        let s = "aé";
        assert_eq!(truncate_bytes(s, 2), "a");
        assert_eq!(truncate_bytes(s, 1), "a");
        assert_eq!(truncate_bytes(s, 3), "aé");
    }
}
