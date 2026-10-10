use std::path::PathBuf;

fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

#[cfg(target_os = "linux")]
pub(crate) fn steam_libraries(home: &std::path::Path) -> Vec<PathBuf> {
    vec![
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ]
}

pub fn candidate_log_dirs() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    // Where the system says Documents is: moved to another drive, or redirected into OneDrive under
    // a localized name (OneDrive\文档 on a Chinese Windows), none of which the fixed paths below see.
    if let Some(docs) = directories::UserDirs::new().and_then(|u| u.document_dir().map(|d| d.to_path_buf())) {
        dirs.push(docs.join("EVE/logs"));
    }

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
        // Every OneDrive (personal, or "OneDrive - Company"), with Documents under whatever name.
        if let Ok(rd) = std::fs::read_dir(&home) {
            for e in rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("OneDrive")) {
                if let Ok(inner) = std::fs::read_dir(e.path()) {
                    for d in inner.flatten() {
                        dirs.push(d.path().join("EVE/logs"));
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(home.join("Documents/EVE/logs"));
        dirs.push(home.join("Library/Application Support/EVE Online/p_drive/User/My Documents/EVE/logs"));
    }

    dirs
}

pub fn chat_logs_dir(configured: &str) -> Option<PathBuf> {
    // A folder set by hand wins when it holds logs; one that does not (moved, mistyped, the wrong
    // level picked) falls back to the search rather than leaving the app with no logs at all.
    configured_logs(configured, "Chatlogs").or_else(|| candidate_log_dirs().into_iter().map(|d| d.join("Chatlogs")).find(|d| d.is_dir()))
}

/// The `sub` folder (Chatlogs or Gamelogs) under a folder set by hand, whichever level was picked:
/// the subfolder itself, `logs`, or the `EVE` folder above it. A folder full of logs counts whatever
/// its name.
fn configured_logs(configured: &str, sub: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if configured.is_empty() {
        return None;
    }
    let p = PathBuf::from(configured);
    if p.ends_with(sub) && p.is_dir() {
        return Some(p);
    }
    for c in [p.join(sub), p.join("logs").join(sub), p.join("EVE").join("logs").join(sub)] {
        if c.is_dir() {
            return Some(c);
        }
    }
    let has_logs = std::fs::read_dir(&p).into_iter().flatten().flatten().any(|e| e.path().extension().is_some_and(|x| x == "txt"));
    has_logs.then_some(p)
}

pub fn game_logs_dir(configured: &str) -> Option<PathBuf> {
    configured_logs(configured, "Gamelogs").or_else(|| candidate_log_dirs().into_iter().map(|d| d.join("Gamelogs")).find(|d| d.is_dir()))
}

/// The current byte length of `path`, read by seeking an open handle to its end. On Windows
/// `DirEntry::metadata().len()` lags by minutes while EVE holds the file open and appends.
pub fn real_len(path: &std::path::Path) -> Option<u64> {
    use std::io::Seek;
    std::fs::File::open(path).ok()?.seek(std::io::SeekFrom::End(0)).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_log_folder_set_by_hand_is_found_at_any_level() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/logpaths-test/文档");
        let chat = root.join("EVE/logs/Chatlogs");
        std::fs::create_dir_all(&chat).unwrap();
        std::fs::write(chat.join("本地_20261010_144512_1.txt"), b"x").unwrap();
        for picked in [root.join("EVE"), root.join("EVE/logs"), chat.clone(), root.clone()] {
            assert_eq!(super::configured_logs(picked.to_str().unwrap(), "Chatlogs"), Some(chat.clone()), "{picked:?}");
        }
        assert_eq!(super::configured_logs(root.join("gone").to_str().unwrap(), "Chatlogs"), None, "a missing folder falls back to the search");
    }

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
        f.write_all(b" world").unwrap();
        f.flush().unwrap();
        assert_eq!(real_len(&path), Some(11));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
