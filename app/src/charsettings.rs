use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

pub fn candidate_eve_roots() -> Vec<PathBuf> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut dirs = Vec::new();

    #[cfg(target_os = "linux")]
    {
        for lib in crate::logpaths::steam_libraries(&home) {
            dirs.push(lib.join(
                "steamapps/compatdata/8500/pfx/drive_c/users/steamuser/AppData/Local/CCP/EVE",
            ));
        }
        if let Ok(user) = std::env::var("USER") {
            dirs.push(home.join(format!(".wine/drive_c/users/{user}/AppData/Local/CCP/EVE")));
        }
    }
    #[cfg(target_os = "windows")]
    {
        dirs.push(home.join("AppData/Local/CCP/EVE"));
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(home.join("Library/Application Support/CCP/EVE"));
    }

    dirs
}

fn is_profile_dir(p: &Path) -> bool {
    p.is_dir()
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("settings_"))
}

/// Newest `core_char_*.dat` mtime anywhere under an installation directory, used to pick the
/// live install when several `*_tranquility` dirs exist.
fn newest_char_file(install: &Path) -> u64 {
    let mut newest = 0;
    let Ok(entries) = std::fs::read_dir(install) else {
        return 0;
    };
    for profile in entries.flatten().map(|e| e.path()).filter(|p| is_profile_dir(p)) {
        let Ok(files) = std::fs::read_dir(&profile) else {
            continue;
        };
        for f in files.flatten() {
            if parse_id(&f.file_name().to_string_lossy(), "core_char_").is_none() {
                continue;
            }
            let secs = f
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            newest = newest.max(secs);
        }
    }
    newest
}

fn install_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with("_tranquility"))
        })
        .collect()
}

/// The directory holding the `settings_*` profile dirs. Accepts an override pointing at either
/// that directory or at one of its `settings_*` children, mirroring how `chat_logs_dir` accepts
/// either the logs root or the Chatlogs dir.
pub fn settings_root(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if is_profile_dir(&p) {
            return p.parent().map(|p| p.to_path_buf());
        }
        if !p.is_dir() {
            return None;
        }
        if !profiles(&p).is_empty() {
            return Some(p);
        }
        return install_dirs(&p).into_iter().max_by_key(|d| newest_char_file(d));
    }
    candidate_eve_roots()
        .iter()
        .flat_map(|root| install_dirs(root))
        .filter(|d| !profiles(d).is_empty())
        .max_by_key(|d| newest_char_file(d))
}

/// Launcher profile names, `settings_` prefix stripped. Directories with `backup` in the name are
/// skipped so a user's hand-made copy is not offered as a live profile.
pub fn profiles(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_profile_dir(p))
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.to_owned()))
        .filter(|n| !n.to_ascii_lowercase().contains("backup"))
        .map(|n| n.trim_start_matches("settings_").to_owned())
        .filter(|n| !n.is_empty())
        .collect();
    out.sort();
    out
}

pub fn profile_dir(root: &Path, profile: &str) -> PathBuf {
    root.join(format!("settings_{profile}"))
}

/// `core_char_123.dat` -> 123. Anything with a suffix past the id (our `_spai_backup_1`, RIFT's
/// `_rift_backup_1`) fails to parse, which is what keeps backups out of every scan.
fn parse_id(name: &str, prefix: &str) -> Option<i64> {
    let rest = name.strip_prefix(prefix)?.strip_suffix(".dat")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

#[derive(Clone, Debug, Default)]
pub struct ProfileScan {
    pub chars: BTreeMap<i64, PathBuf>,
    pub accounts: BTreeMap<i64, PathBuf>,
}

pub fn scan(root: &Path, profile: &str) -> ProfileScan {
    let mut out = ProfileScan::default();
    let Ok(entries) = std::fs::read_dir(profile_dir(root, profile)) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(id) = parse_id(&name, "core_char_") {
            out.chars.insert(id, path);
        } else if let Some(id) = parse_id(&name, "core_user_") {
            out.accounts.insert(id, path);
        }
    }
    out
}

/// Every profile scanned at once, for "which characters exist anywhere" and for mtime pairing.
pub fn scan_all(root: &Path) -> BTreeMap<String, ProfileScan> {
    profiles(root).into_iter().map(|p| { let s = scan(root, &p); (p, s) }).collect()
}

pub fn modified_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// EVE names both its chat and game logs `..._<characterId>.txt` and writes `Listener: <name>` in
/// the header, so every character that has played on this machine can be named offline. Ids below
/// this are the time component of a log with no character suffix.
const MIN_CHARACTER_ID: i64 = 90_000_000;

fn id_from_log_name(name: &str) -> Option<i64> {
    let id: i64 = name.strip_suffix(".txt")?.rsplit('_').next()?.parse().ok()?;
    (id >= MIN_CHARACTER_ID).then_some(id)
}

/// Chat logs are UTF-16LE, game logs UTF-8. Only the header is needed, so this reads a prefix
/// rather than whole log files, some of which are megabytes.
fn listener_of(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut buf = [0u8; 4096];
    let read = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
    let bytes = &buf[..read];
    let units: Vec<u16> =
        bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    for text in [String::from_utf16_lossy(&units), String::from_utf8_lossy(bytes).into_owned()] {
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}').trim();
            if let Some(name) = line.strip_prefix("Listener:").map(str::trim) {
                if !name.is_empty() {
                    return Some(name.to_owned());
                }
            }
        }
    }
    None
}

/// Local names for the requested character ids, read from the newest log each one appears in.
/// Only one file per wanted id is opened, so a directory of thousands of logs stays cheap.
pub fn names_from_logs(logs_configured: &str, wanted: &[i64]) -> BTreeMap<i64, String> {
    let mut newest: BTreeMap<i64, (String, PathBuf)> = BTreeMap::new();
    let dirs = [
        crate::logpaths::chat_logs_dir(logs_configured),
        crate::logpaths::game_logs_dir(logs_configured),
    ];
    for dir in dirs.into_iter().flatten() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let file = e.file_name().to_string_lossy().to_string();
            let Some(id) = id_from_log_name(&file) else {
                continue;
            };
            if !wanted.contains(&id) {
                continue;
            }
            // Log names embed a sortable timestamp, so the lexicographic max is the newest.
            if newest.get(&id).is_none_or(|(prev, _)| *prev < file) {
                newest.insert(id, (file, e.path()));
            }
        }
    }
    newest.into_iter().filter_map(|(id, (_, path))| listener_of(&path).map(|n| (id, n))).collect()
}

#[derive(Clone, Debug)]
pub struct CopyPlan {
    pub source_char: i64,
    pub source_profile: String,
    pub dest_chars: Vec<i64>,
    pub dest_profile: String,
    pub source_account: Option<i64>,
    /// Destination accounts to overwrite with the source account file. Empty when the source
    /// account is unknown or no destination's account could be resolved.
    pub dest_accounts: Vec<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct CopyReport {
    pub char_files: usize,
    pub account_files: usize,
    pub backups: Vec<PathBuf>,
    pub skipped_same_file: usize,
}

/// First free `<stem>_spai_backup_N.dat`. Unbounded and never pruned, matching RIFT's scheme so a
/// user who has both tools sees the same shape of history.
fn next_backup(target: &Path) -> Option<PathBuf> {
    let dir = target.parent()?;
    let stem = target.file_stem()?.to_str()?;
    for n in 1..10_000 {
        let candidate = dir.join(format!("{stem}_spai_backup_{n}.dat"));
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn replicate(
    from: &Path,
    targets: &[PathBuf],
    report: &mut CopyReport,
) -> Result<usize, String> {
    let mut copied = 0;
    for target in targets {
        if target == from {
            report.skipped_same_file += 1;
            continue;
        }
        if target.exists() {
            let backup = next_backup(target)
                .ok_or_else(|| format!("could not pick a backup name for {}", target.display()))?;
            std::fs::copy(target, &backup).map_err(|e| {
                format!("backing up {} failed: {e}", target.display())
            })?;
            report.backups.push(backup);
        }
        std::fs::copy(from, target)
            .map_err(|e| format!("writing {} failed: {e}", target.display()))?;
        copied += 1;
    }
    Ok(copied)
}

/// Copies the source character's settings onto every destination. Everything is validated before
/// the first write, so a bad plan fails without touching the destination files. Once writing
/// starts there is no rollback, which is what the backups are for.
pub fn copy(root: &Path, plan: &CopyPlan) -> Result<CopyReport, String> {
    let src_dir = profile_dir(root, &plan.source_profile);
    let dst_dir = profile_dir(root, &plan.dest_profile);

    let src_char = src_dir.join(format!("core_char_{}.dat", plan.source_char));
    if !src_char.is_file() {
        return Err(format!(
            "no settings file for the source character in profile \"{}\"",
            plan.source_profile
        ));
    }
    let src_account = match plan.source_account {
        Some(id) => {
            let p = src_dir.join(format!("core_user_{id}.dat"));
            if !p.is_file() && !plan.dest_accounts.is_empty() {
                return Err(format!(
                    "no account settings file for account {id} in profile \"{}\"",
                    plan.source_profile
                ));
            }
            Some(p)
        }
        None => None,
    };

    std::fs::create_dir_all(&dst_dir)
        .map_err(|e| format!("could not create {}: {e}", dst_dir.display()))?;

    let mut char_targets: Vec<PathBuf> =
        plan.dest_chars.iter().map(|id| dst_dir.join(format!("core_char_{id}.dat"))).collect();
    char_targets.sort();
    char_targets.dedup();

    let mut account_targets: Vec<PathBuf> =
        plan.dest_accounts.iter().map(|id| dst_dir.join(format!("core_user_{id}.dat"))).collect();
    account_targets.sort();
    account_targets.dedup();

    let mut report = CopyReport::default();
    report.char_files = replicate(&src_char, &char_targets, &mut report)?;
    if let Some(src_account) = src_account {
        if !account_targets.is_empty() {
            report.account_files = replicate(&src_account, &account_targets, &mut report)?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tree(tag: &str) -> Tmp {
        let dir = std::env::temp_dir()
            .join(format!("evespai-charsettings-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        for profile in ["settings_Default", "settings_Alt", "settings_Default_backup"] {
            std::fs::create_dir_all(dir.join(profile)).unwrap();
        }
        let d = dir.join("settings_Default");
        std::fs::write(d.join("core_char_100.dat"), b"source-char").unwrap();
        std::fs::write(d.join("core_char_200.dat"), b"dest-char-old").unwrap();
        std::fs::write(d.join("core_char_300.dat"), b"other-char").unwrap();
        std::fs::write(d.join("core_user_10.dat"), b"source-account").unwrap();
        std::fs::write(d.join("core_user_20.dat"), b"dest-account-old").unwrap();
        std::fs::write(d.join("core_char_100_rift_backup_3.dat"), b"rift decoy").unwrap();
        std::fs::write(d.join("core_char_100_spai_backup_1.dat"), b"our decoy").unwrap();
        std::fs::write(d.join("core_public__.yaml"), b"not a settings file").unwrap();
        Tmp(dir)
    }

    #[test]
    fn log_names_carry_character_ids() {
        assert_eq!(id_from_log_name("20260802_145000_2124597497.txt"), Some(2124597497));
        assert_eq!(
            id_from_log_name("west.imperium_20260801_134424_2124364410.txt"),
            Some(2124364410)
        );
        // A gamelog with no character suffix ends in the time, which is not a character id.
        assert_eq!(id_from_log_name("20260802_045113.txt"), None);
        assert_eq!(id_from_log_name("notalog.log"), None);
    }

    #[test]
    fn listener_read_from_both_encodings() {
        let dir = std::env::temp_dir().join(format!("evespai-listener-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let game = dir.join("20260802_145000_2124597497.txt");
        std::fs::write(
            &game,
            "------------\n  Gamelog\n  Listener: Projection Issues\n  Session Started: x\n",
        )
        .unwrap();
        assert_eq!(listener_of(&game).as_deref(), Some("Projection Issues"));

        let chat = dir.join("local_20260802_145000_2119400938.txt");
        let text = "\u{feff}---------\n  Channel Name:    Local\n  Listener:        Amryu\n";
        let utf16: Vec<u8> =
            text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        std::fs::write(&chat, &utf16).unwrap();
        assert_eq!(listener_of(&chat).as_deref(), Some("Amryu"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_id_rejects_backups() {
        assert_eq!(parse_id("core_char_123.dat", "core_char_"), Some(123));
        assert_eq!(parse_id("core_char_123_rift_backup_1.dat", "core_char_"), None);
        assert_eq!(parse_id("core_char_123_spai_backup_9.dat", "core_char_"), None);
        assert_eq!(parse_id("core_user__.dat", "core_user_"), None);
        assert_eq!(parse_id("core_char_.dat", "core_char_"), None);
    }

    #[test]
    fn scan_finds_live_files_only() {
        let t = tree("scan");
        let s = scan(&t.0, "Default");
        assert_eq!(s.chars.keys().copied().collect::<Vec<_>>(), vec![100, 200, 300]);
        assert_eq!(s.accounts.keys().copied().collect::<Vec<_>>(), vec![10, 20]);
    }

    #[test]
    fn profiles_skip_backup_dirs() {
        let t = tree("profiles");
        assert_eq!(profiles(&t.0), vec!["Alt".to_owned(), "Default".to_owned()]);
    }

    #[test]
    fn copy_writes_backup_and_replaces() {
        let t = tree("copy");
        let plan = CopyPlan {
            source_char: 100,
            source_profile: "Default".to_owned(),
            dest_chars: vec![200],
            dest_profile: "Default".to_owned(),
            source_account: Some(10),
            dest_accounts: vec![20],
        };
        let report = copy(&t.0, &plan).unwrap();
        assert_eq!(report.char_files, 1);
        assert_eq!(report.account_files, 1);
        let d = t.0.join("settings_Default");
        assert_eq!(std::fs::read(d.join("core_char_200.dat")).unwrap(), b"source-char");
        assert_eq!(std::fs::read(d.join("core_user_20.dat")).unwrap(), b"source-account");
        // _spai_backup_1 for char 200 is free, char 100's decoy does not shift it.
        assert_eq!(
            std::fs::read(d.join("core_char_200_spai_backup_1.dat")).unwrap(),
            b"dest-char-old"
        );
        assert_eq!(
            std::fs::read(d.join("core_user_20_spai_backup_1.dat")).unwrap(),
            b"dest-account-old"
        );
        assert_eq!(std::fs::read(d.join("core_char_300.dat")).unwrap(), b"other-char");
    }

    #[test]
    fn backup_index_skips_taken_slots() {
        let t = tree("backupidx");
        let plan = CopyPlan {
            source_char: 200,
            source_profile: "Default".to_owned(),
            dest_chars: vec![100],
            dest_profile: "Default".to_owned(),
            source_account: None,
            dest_accounts: vec![],
        };
        copy(&t.0, &plan).unwrap();
        let d = t.0.join("settings_Default");
        // _spai_backup_1 was already on disk, so the new backup lands at 2.
        assert_eq!(std::fs::read(d.join("core_char_100_spai_backup_1.dat")).unwrap(), b"our decoy");
        assert_eq!(
            std::fs::read(d.join("core_char_100_spai_backup_2.dat")).unwrap(),
            b"source-char"
        );
    }

    #[test]
    fn copy_skips_self_and_creates_target_profile() {
        let t = tree("selfskip");
        let plan = CopyPlan {
            source_char: 100,
            source_profile: "Default".to_owned(),
            dest_chars: vec![100, 200],
            dest_profile: "Fresh".to_owned(),
            source_account: None,
            dest_accounts: vec![],
        };
        let report = copy(&t.0, &plan).unwrap();
        // Different profile, so copying onto the source id is a real, distinct target.
        assert_eq!(report.char_files, 2);
        assert_eq!(report.skipped_same_file, 0);
        let fresh = t.0.join("settings_Fresh");
        assert_eq!(std::fs::read(fresh.join("core_char_100.dat")).unwrap(), b"source-char");
        assert_eq!(std::fs::read(fresh.join("core_char_200.dat")).unwrap(), b"source-char");

        let same = CopyPlan { dest_profile: "Default".to_owned(), ..plan };
        let report = copy(&t.0, &same).unwrap();
        assert_eq!(report.skipped_same_file, 1);
        assert_eq!(report.char_files, 1);
    }

    #[test]
    fn missing_source_aborts_before_writing() {
        let t = tree("nosource");
        let plan = CopyPlan {
            source_char: 999,
            source_profile: "Default".to_owned(),
            dest_chars: vec![200],
            dest_profile: "Default".to_owned(),
            source_account: None,
            dest_accounts: vec![],
        };
        assert!(copy(&t.0, &plan).is_err());
        let d = t.0.join("settings_Default");
        assert_eq!(std::fs::read(d.join("core_char_200.dat")).unwrap(), b"dest-char-old");
        assert!(!d.join("core_char_200_spai_backup_1.dat").exists());
    }

    #[test]
    fn settings_root_accepts_profile_dir_override() {
        let t = tree("override");
        let via_profile = settings_root(t.0.join("settings_Default").to_str().unwrap()).unwrap();
        assert_eq!(via_profile, t.0);
        let via_root = settings_root(t.0.to_str().unwrap()).unwrap();
        assert_eq!(via_root, t.0);
        assert!(settings_root(t.0.join("nope").to_str().unwrap()).is_none());
    }

    #[test]
    fn settings_root_picks_newest_install() {
        let t = tree("install");
        let root = t.0.join("root");
        for install in ["a_tq_tranquility", "b_tq_tranquility"] {
            std::fs::create_dir_all(root.join(install).join("settings_Default")).unwrap();
        }
        std::fs::write(root.join("a_tq_tranquility/settings_Default/core_char_1.dat"), b"a")
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("b_tq_tranquility/settings_Default/core_char_2.dat"), b"b")
            .unwrap();
        assert_eq!(settings_root(root.to_str().unwrap()).unwrap(), root.join("b_tq_tranquility"));
    }
}
