use base64::Engine;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EveClient {
    pub pid: u32,
    pub character_id: Option<i64>,
    pub account_id: Option<i64>,
    pub profile: Option<String>,
}

/// `None` means the platform could not be queried at all, which the UI treats as "unverified"
/// rather than "closed". `Some(vec![])` means no client is running.
pub type Clients = Option<Vec<EveClient>>;

/// The launcher hands the client `/LauncherData=<base64>` holding
/// `eve-online:tranquility::<accountId>:<characterId>`. Only quick-start launches carry it, so a
/// client without it still counts for the running check, it just teaches us nothing.
pub fn parse_launcher_data(b64: &str) -> Option<(i64, i64)> {
    let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    let text = String::from_utf8(raw).ok()?;
    let mut parts = text.split(':');
    if parts.next()? != "eve-online" {
        return None;
    }
    let fields: Vec<&str> = parts.collect();
    let account = fields.iter().rev().nth(1)?.parse().ok()?;
    let character = fields.last()?.parse().ok()?;
    Some((account, character))
}

/// Recognises the game client from its argv. `eve_crashmon.exe` shares the install dir and often
/// outlives the client, so a bare "exefile" match would keep the copy gate closed after EVE exits.
fn client_from_args(pid: u32, args: &str) -> Option<EveClient> {
    let lower = args.to_ascii_lowercase();
    if !lower.contains("exefile.exe") {
        return None;
    }
    if !lower.contains("/server:") && !lower.contains("/launcherdata=") {
        return None;
    }
    let field = |prefix: &str| -> Option<String> {
        args.split_whitespace()
            .find_map(|tok| tok.strip_prefix(prefix))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let launcher = field("/LauncherData=").and_then(|v| parse_launcher_data(&v));
    let character_id = field("/autoSelectCharacter:")
        .and_then(|v| v.parse().ok())
        .or(launcher.map(|(_, c)| c));
    Some(EveClient {
        pid,
        character_id,
        account_id: launcher.map(|(a, _)| a),
        profile: field("/settingsprofile="),
    })
}

#[cfg(target_os = "linux")]
pub fn running_clients() -> Clients {
    let entries = std::fs::read_dir("/proc").ok()?;
    let mut out = Vec::new();
    for e in entries.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(raw) = std::fs::read(e.path().join("cmdline")) else {
            continue;
        };
        let args = String::from_utf8_lossy(&raw).replace('\0', " ");
        if let Some(c) = client_from_args(pid, &args) {
            out.push(c);
        }
    }
    Some(out)
}

#[cfg(target_os = "macos")]
pub fn running_clients() -> Clients {
    let out = std::process::Command::new("ps").args(["-axww", "-o", "pid=,args="]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    Some(
        text.lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let (pid, args) = line.split_once(char::is_whitespace)?;
                client_from_args(pid.parse().ok()?, args)
            })
            .collect(),
    )
}

/// Windows: the process list comes from a plain toolhelp snapshot, and the command lines (needed
/// only for the association harvest) from a hidden CIM query. If the query fails the gate still
/// works, we just learn nothing.
#[cfg(target_os = "windows")]
pub fn running_clients() -> Clients {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let pids = windows_exefile_pids()?;
    if pids.is_empty() {
        return Some(Vec::new());
    }
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='exefile.exe'\" | \
             ForEach-Object { \"$($_.ProcessId) $($_.CommandLine)\" }",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(out) = out {
        let text = String::from_utf8_lossy(&out.stdout);
        let parsed: Vec<EveClient> = text
            .lines()
            .filter_map(|line| {
                let (pid, args) = line.trim().split_once(char::is_whitespace)?;
                client_from_args(pid.parse().ok()?, args)
            })
            .collect();
        if !parsed.is_empty() {
            return Some(parsed);
        }
    }
    Some(
        pids.into_iter()
            .map(|pid| EveClient { pid, character_id: None, account_id: None, profile: None })
            .collect(),
    )
}

#[cfg(target_os = "windows")]
fn windows_exefile_pids() -> Option<Vec<u32>> {
    use core::ffi::c_void;

    #[repr(C)]
    struct ProcessEntry32W {
        size: u32,
        usage: u32,
        process_id: u32,
        default_heap_id: usize,
        module_id: u32,
        threads: u32,
        parent_process_id: u32,
        pri_class_base: i32,
        flags: u32,
        exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> *mut c_void;
        fn Process32FirstW(snapshot: *mut c_void, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snapshot: *mut c_void, entry: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    let invalid = usize::MAX as *mut c_void;

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == invalid {
            return None;
        }
        let mut entry: ProcessEntry32W = core::mem::zeroed();
        entry.size = core::mem::size_of::<ProcessEntry32W>() as u32;
        let mut pids = Vec::new();
        let mut ok = Process32FirstW(snapshot, &mut entry);
        while ok != 0 {
            let len = entry.exe_file.iter().position(|c| *c == 0).unwrap_or(entry.exe_file.len());
            let name = String::from_utf16_lossy(&entry.exe_file[..len]);
            if name.eq_ignore_ascii_case("exefile.exe") {
                pids.push(entry.process_id);
            }
            ok = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        Some(pids)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn running_clients() -> Clients {
    None
}

const POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// Launcher profile last seen on a running client's argv, used to preselect the profile to copy.
pub const LAST_PROFILE_KEY: &str = "eve.last_profile";

/// Watches for running clients and learns character-to-account associations while the user plays,
/// which is the only time either signal exists. The copy UI reads the shared client list to decide
/// whether to gate itself.
pub fn spawn_poller(
    shared: std::sync::Arc<std::sync::Mutex<Clients>>,
    configured: std::sync::Arc<std::sync::Mutex<String>>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let store = crate::store::Store::open().ok();
        let mut prev: std::collections::HashMap<std::path::PathBuf, u64> = Default::default();
        let mut baseline = true;
        loop {
            let clients = running_clients();
            if let Some(store) = &store {
                if let Some(list) = &clients {
                    for c in list {
                        if let (Some(char_id), Some(account_id)) = (c.character_id, c.account_id) {
                            store.set_char_account(
                                char_id,
                                account_id,
                                crate::store::AssocSource::Client,
                            );
                        }
                        // The copy UI can only run with EVE closed, so the profile the user
                        // actually plays has to be remembered while a client is up.
                        if let Some(profile) = &c.profile {
                            store.kv_set(LAST_PROFILE_KEY, profile);
                        }
                    }
                }
                let cfg = configured.lock().map(|c| c.clone()).unwrap_or_default();
                pair_by_mtime(store, &cfg, &mut prev, &mut baseline);
            }

            let was_running = shared
                .lock()
                .map(|c| c.as_ref().is_some_and(|l| !l.is_empty()))
                .unwrap_or(false);
            let now_running = clients.as_ref().is_some_and(|l| !l.is_empty());
            if let Ok(mut slot) = shared.lock() {
                *slot = clients;
            }
            if was_running != now_running {
                ctx.request_repaint();
            }
            std::thread::sleep(POLL);
        }
    });
}

/// EVE writes a character's file and its account's file together when that character logs out. When
/// exactly one of each changed between two polls the pairing is unambiguous; anything busier (two
/// clients closing at once) is discarded rather than guessed at.
fn pair_by_mtime(
    store: &crate::store::Store,
    configured: &str,
    prev: &mut std::collections::HashMap<std::path::PathBuf, u64>,
    baseline: &mut bool,
) {
    let Some(root) = crate::charsettings::settings_root(configured) else {
        return;
    };
    let mut current = std::collections::HashMap::new();
    let mut changed_chars = Vec::new();
    let mut changed_accounts = Vec::new();
    for scan in crate::charsettings::scan_all(&root).values() {
        for (kind, id, path) in scan
            .chars
            .iter()
            .map(|(id, p)| (0u8, *id, p))
            .chain(scan.accounts.iter().map(|(id, p)| (1u8, *id, p)))
        {
            let mtime = crate::charsettings::modified_secs(path);
            current.insert(path.clone(), mtime);
            if prev.get(path).is_some_and(|old| *old != mtime) {
                if kind == 0 {
                    changed_chars.push(id);
                } else {
                    changed_accounts.push(id);
                }
            }
        }
    }
    *prev = current;
    if *baseline {
        *baseline = false;
        return;
    }
    changed_chars.sort_unstable();
    changed_chars.dedup();
    changed_accounts.sort_unstable();
    changed_accounts.dedup();
    if changed_chars.len() == 1 && changed_accounts.len() == 1 {
        store.set_char_account(
            changed_chars[0],
            changed_accounts[0],
            crate::store::AssocSource::Mtime,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = "C:\\CCP\\EVE\\tq\\bin64\\exefile.exe /noconsole \
/server:tranquility.servers.eveonline.com /ssoToken=eyJhbGciOiJSUzI1NiJ9.e30.sig \
/refreshToken=hZp30XyOf0SB0lCVsbm/mg== /settingsprofile=Default /language=en \
/LauncherData=ZXZlLW9ubGluZTp0cmFucXVpbGl0eTo6MTg2OTQwMjY6MjExOTQwMDkzOA== \
/triplatform=dx11 /steamUser /autoSelectCharacter:2119400938";

    #[test]
    fn parses_real_launcher_data() {
        assert_eq!(
            parse_launcher_data("ZXZlLW9ubGluZTp0cmFucXVpbGl0eTo6MTg2OTQwMjY6MjExOTQwMDkzOA=="),
            Some((18694026, 2119400938))
        );
    }

    #[test]
    fn rejects_junk_launcher_data() {
        assert_eq!(parse_launcher_data(""), None);
        assert_eq!(parse_launcher_data("not base64 at all !!"), None);
        // Valid base64, wrong product.
        let other = base64::engine::general_purpose::STANDARD.encode("eve-vanguard:tq::1:2");
        assert_eq!(parse_launcher_data(&other), None);
        let short = base64::engine::general_purpose::STANDARD.encode("eve-online");
        assert_eq!(parse_launcher_data(&short), None);
        let nonnumeric = base64::engine::general_purpose::STANDARD.encode("eve-online:tq::a:b");
        assert_eq!(parse_launcher_data(&nonnumeric), None);
    }

    #[test]
    fn recognises_the_game_client() {
        let c = client_from_args(1094367, REAL).unwrap();
        assert_eq!(c.account_id, Some(18694026));
        assert_eq!(c.character_id, Some(2119400938));
        assert_eq!(c.profile.as_deref(), Some("Default"));
    }

    #[test]
    fn ignores_crashmon_and_launcher() {
        let crashmon = "C:\\CCP\\EVE\\tq\\bin64\\eve_crashmon.exe --no-rate-limit \
--database=C:\\users\\steamuser\\AppData\\Local\\CCP\\EVE\\";
        assert!(client_from_args(1, crashmon).is_none());
        // exefile with no server/launcher args is not a live game client.
        assert!(client_from_args(2, "Z:\\path\\exefile.exe /help").is_none());
        assert!(client_from_args(3, "/usr/bin/eve-spai --overlay").is_none());
    }

    #[test]
    fn non_quickstart_client_still_counts() {
        let plain = "C:\\CCP\\EVE\\tq\\bin64\\exefile.exe /noconsole \
/server:tranquility.servers.eveonline.com /settingsprofile=Alt";
        let c = client_from_args(7, plain).unwrap();
        assert_eq!(c.pid, 7);
        assert_eq!(c.account_id, None);
        assert_eq!(c.character_id, None);
        assert_eq!(c.profile.as_deref(), Some("Alt"));
    }
}
