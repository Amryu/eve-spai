//! Locating EVE Online's chat-log directory across platforms.
//!
//! The candidate paths below are EVE's own static install locations (Documents,
//! Steam Proton prefix for app id 8500, the macOS Wine wrapper). A user-set path
//! in Settings overrides detection.

use std::path::PathBuf;

fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

/// Common Steam library roots on Linux (default, Flatpak, alternate share path).
#[cfg(target_os = "linux")]
fn steam_libraries(home: &std::path::Path) -> Vec<PathBuf> {
    vec![
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ]
}

/// Candidate `EVE/logs` directories for this platform.
pub fn candidate_log_dirs() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut dirs = Vec::new();

    #[cfg(target_os = "linux")]
    {
        dirs.push(home.join("Documents/EVE/logs"));
        for lib in steam_libraries(&home) {
            dirs.push(lib.join(
                "steamapps/compatdata/8500/pfx/drive_c/users/steamuser/Documents/EVE/logs",
            ));
        }
    }
    #[cfg(target_os = "windows")]
    {
        dirs.push(home.join("Documents/EVE/logs"));
        dirs.push(home.join("OneDrive/Documents/EVE/logs"));
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(home.join("Documents/EVE/logs"));
        dirs.push(home.join("Library/Application Support/EVE Online/p_drive/User/My Documents/EVE/logs"));
    }

    dirs
}

/// Resolve the `Chatlogs` directory: honour a configured path (which may point at
/// either `EVE/logs` or directly at `Chatlogs`), else auto-detect.
pub fn chat_logs_dir(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if p.ends_with("Chatlogs") && p.is_dir() {
            return Some(p);
        }
        let cl = p.join("Chatlogs");
        if cl.is_dir() {
            return Some(cl);
        }
        return p.is_dir().then_some(p);
    }
    candidate_log_dirs()
        .into_iter()
        .map(|d| d.join("Chatlogs"))
        .find(|d| d.is_dir())
}

/// Resolve the `Gamelogs` directory (combat logs), honouring a configured path.
pub fn game_logs_dir(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if p.ends_with("Gamelogs") && p.is_dir() {
            return Some(p);
        }
        let gl = p.join("Gamelogs");
        if gl.is_dir() {
            return Some(gl);
        }
        return None;
    }
    candidate_log_dirs()
        .into_iter()
        .map(|d| d.join("Gamelogs"))
        .find(|d| d.is_dir())
}

/// The real current byte length of `path`, queried from an OPEN handle (its true end-of-file), not
/// the directory entry. On Windows the directory entry's size is updated lazily while another
/// process (EVE) holds the file open and appends, so `DirEntry::metadata().len()` stays stale for
/// minutes; seeking a freshly-opened handle to the end always sees the real size, which is how the
/// log watchers detect new lines without lagging behind on Windows.
pub fn real_len(path: &std::path::Path) -> Option<u64> {
    use std::io::Seek;
    std::fs::File::open(path).ok()?.seek(std::io::SeekFrom::End(0)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn real_len_reflects_appends() {
        let dir = std::env::temp_dir().join(format!("evespai-reallen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chat.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"hello").unwrap();
        f.flush().unwrap();
        assert_eq!(real_len(&path), Some(5));
        // Append through the SAME open handle (as EVE does) and confirm the real size grows.
        f.write_all(b" world").unwrap();
        f.flush().unwrap();
        assert_eq!(real_len(&path), Some(11));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
