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

pub fn spawn_check(state: SharedUpdate, skip_version: String, ctx: egui::Context) {
    if REPO.starts_with("OWNER/") {
        return;
    }
    std::thread::spawn(move || {
        let Some(client) = http() else { return };
        let mut req = client
            .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
            .header("Accept", "application/vnd.github+json");
        if let Some(t) = token() {
            req = req.header("Authorization", format!("token {t}"));
        }
        let Ok(resp) = req.send().and_then(|r| r.error_for_status()) else { return };
        let Ok(json) = resp.json::<serde_json::Value>() else { return };
        let tag = json["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_owned();
        if tag.is_empty() || tag == skip_version || !is_newer(&tag, current()) {
            return;
        }
        let html_url = json["html_url"].as_str().unwrap_or("").to_owned();
        let asset_api_url = json["assets"].as_array().and_then(|a| {
            a.iter().find(|x| x["name"].as_str() == Some(asset_name())).and_then(|x| x["url"].as_str())
        }).map(|s| s.to_owned());
        state.lock().unwrap().available = Some(Available { version: tag, html_url, asset_api_url });
        ctx.request_repaint();
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
