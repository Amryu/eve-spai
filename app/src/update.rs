use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REPO: &str = "Amryu/eve-spai";

/// How often the app looks for a new release while it is running.
pub const CHECK_EVERY: Duration = Duration::from_secs(3600);

/// Passed to the relaunched process so it knows to wait for the old one's single-instance lock to
/// come free instead of assuming another copy is already up and bailing out.
pub const RESTART_FLAG: &str = "--restarted";

static RESTART: AtomicBool = AtomicBool::new(false);

/// Restart on the way out rather than here: the single-instance lock is held for the life of the
/// process, so a new copy can only come up once this one is gone.
pub fn request_restart() {
    RESTART.store(true, Ordering::SeqCst);
}

pub fn restart_requested() -> bool {
    RESTART.load(Ordering::SeqCst)
}

pub fn relaunch() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe).arg(RESTART_FLAG).spawn()?;
    Ok(())
}

/// The elevated helper runs the app as `--apply-update <asset_url>`: it does the swap with admin
/// rights, then exits.
pub const UPDATE_FLAG: &str = "--apply-update";

pub fn apply_update_arg() -> Option<String> {
    let mut args = std::env::args();
    while let Some(a) = args.next() {
        if a == UPDATE_FLAG {
            return args.next();
        }
    }
    None
}

/// A machine-wide install (Program Files) is not writable without elevation, and the update swaps
/// the exe in place, so probe the exe's directory.
pub fn update_needs_admin() -> bool {
    #[cfg(windows)]
    {
        let Ok(exe) = std::env::current_exe() else { return false };
        let Some(dir) = exe.parent() else { return false };
        let probe = dir.join(".eve-spai-write-probe");
        match std::fs::File::create(&probe) {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                false
            }
            Err(e) => e.kind() == std::io::ErrorKind::PermissionDenied,
        }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Relaunch elevated (UAC prompt) to perform the swap, waiting for it and reporting whether it
/// applied. Windows only.
#[cfg(windows)]
pub fn elevated_update(asset_api_url: &str) -> anyhow::Result<()> {
    use anyhow::{bail, Context};
    let exe = std::env::current_exe().context("locating current executable")?;
    // Start-Process -Verb RunAs raises the UAC prompt; -Wait -PassThru lets us read the helper's
    // exit code back out. A declined prompt throws, so PowerShell exits non-zero.
    let script = format!(
        "$ErrorActionPreference='Stop';\
         $p=Start-Process -FilePath '{exe}' -ArgumentList '{flag}','{url}' -Verb RunAs -Wait -PassThru;\
         exit $p.ExitCode",
        exe = exe.display(),
        flag = UPDATE_FLAG,
        url = asset_api_url,
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .context("launching the elevated updater")?;
    if status.success() {
        Ok(())
    } else {
        bail!("the update needs administrator rights, and the prompt was declined or failed");
    }
}

#[cfg(not(windows))]
pub fn elevated_update(_asset_api_url: &str) -> anyhow::Result<()> {
    anyhow::bail!("elevated update is Windows-only")
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Clone)]
pub struct Available {
    pub version: String,
    pub html_url: String,
    pub asset_api_url: Option<String>,
}

#[derive(Clone, Default)]
pub struct UpdateState {
    pub available: Option<Available>,
    pub installing: bool,
    pub done: bool,
    pub error: Option<String>,
    /// A manual "check for updates" is in flight.
    pub checking: bool,
    /// The last manual check found nothing newer.
    pub up_to_date: bool,
    /// The last manual check couldn't reach GitHub.
    pub check_failed: Option<String>,
}

pub type SharedUpdate = Arc<Mutex<UpdateState>>;

fn token() -> Option<String> {
    std::env::var("EVE_SPAI_UPDATE_TOKEN").ok().filter(|t| !t.is_empty())
}

fn asset_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "eve-spai-windows-x86_64.exe"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "eve-spai-macos-aarch64"
        } else {
            "eve-spai-macos-x86_64"
        }
    } else {
        "eve-spai-linux-x86_64"
    }
}

fn http() -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder().user_agent("eve-spai").timeout(Duration::from_secs(20)).build().ok()
}

/// `announce` marks a user-initiated check: it reports "up to date" and connection failures back to
/// the UI. The automatic hourly check passes false and stays silent unless there's an update.
pub fn spawn_check(state: SharedUpdate, skip_version: String, announce: bool, ctx: egui::Context) {
    if REPO.starts_with("OWNER/") {
        return;
    }
    if announce {
        let mut s = state.lock().unwrap();
        s.checking = true;
        s.up_to_date = false;
        s.check_failed = None;
    }
    std::thread::spawn(move || {
        let finish = |state: &SharedUpdate| {
            if announce {
                state.lock().unwrap().checking = false;
            }
            ctx.request_repaint();
        };
        let fail = |state: &SharedUpdate, msg: &str| {
            if announce {
                let mut s = state.lock().unwrap();
                s.checking = false;
                s.check_failed = Some(msg.to_owned());
            }
            ctx.request_repaint();
        };

        let Some(client) = http() else { return fail(&state, "couldn't start the HTTP client") };
        let mut req = client
            .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
            .header("Accept", "application/vnd.github+json");
        if let Some(t) = token() {
            req = req.header("Authorization", format!("token {t}"));
        }
        let resp = match req.send().and_then(|r| r.error_for_status()) {
            Ok(r) => r,
            Err(e) => return fail(&state, &e.to_string()),
        };
        let json = match resp.json::<serde_json::Value>() {
            Ok(j) => j,
            Err(e) => return fail(&state, &e.to_string()),
        };
        let tag = json["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_owned();
        if tag.is_empty() || tag == skip_version || !is_newer(&tag, current()) {
            if announce {
                state.lock().unwrap().up_to_date = true;
            }
            return finish(&state);
        }
        let html_url = json["html_url"].as_str().unwrap_or("").to_owned();
        let asset_api_url = json["assets"].as_array().and_then(|a| {
            a.iter().find(|x| x["name"].as_str() == Some(asset_name())).and_then(|x| x["url"].as_str())
        }).map(|s| s.to_owned());
        state.lock().unwrap().available = Some(Available { version: tag, html_url, asset_api_url });
        finish(&state);
    });
}

fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.').map(|p| p.split('-').next().unwrap_or("").parse::<u64>().unwrap_or(0)).collect()
    };
    parse(a) > parse(b)
}

pub fn download_and_replace(asset_api_url: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let client = reqwest::blocking::Client::builder()
        .user_agent("eve-spai")
        .timeout(Duration::from_secs(180))
        .build()?;
    let mut req = client.get(asset_api_url).header("Accept", "application/octet-stream");
    if let Some(t) = token() {
        req = req.header("Authorization", format!("token {t}"));
    }
    let bytes = req.send()?.error_for_status()?.bytes()?;

    let exe = std::env::current_exe().context("locating current executable")?;
    let new_path = exe.with_extension("new");
    std::fs::write(&new_path, &bytes).context("writing new binary")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new_path, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(windows)]
    {
        // A running .exe can be renamed (not deleted), so move it aside first.
        let old = exe.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).context("moving old binary aside")?;
        std::fs::rename(&new_path, &exe).context("installing new binary")?;
        // The old image is the still-running process, so it can't be deleted now. `cleanup_old`
        // clears it on next start when the dir is writable; schedule a reboot-delete as a fallback
        // for machine-wide installs where the non-elevated relaunch can't remove it.
        schedule_delete_on_reboot(&old);
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(&new_path, &exe).context("replacing binary")?;
    }
    Ok(())
}

pub fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

/// Mark a file for deletion at the next reboot (`MoveFileExW(path, NULL, DELAY_UNTIL_REBOOT)`), the
/// only way to shed a file a running process holds open and a non-elevated relaunch can't remove.
#[cfg(windows)]
fn schedule_delete_on_reboot(path: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    const MOVEFILE_DELAY_UNTIL_REBOOT: u32 = 0x4;
    unsafe {
        MoveFileExW(wide.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT);
    }
}
