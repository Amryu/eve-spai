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

/// The elevated helper runs the app as `--apply-update <downloaded file> <sha256>`: it checks the
/// file is still what was verified, does the swap with admin rights, then exits.
pub const UPDATE_FLAG: &str = "--apply-update";

pub fn apply_update_arg() -> Option<(String, String)> {
    let mut args = std::env::args();
    while let Some(a) = args.next() {
        if a == UPDATE_FLAG {
            return Some((args.next()?, args.next()?));
        }
    }
    None
}

/// The elevated side of `elevated_update`: the file the user's side downloaded and verified, checked
/// again here, since anything running as the user could have changed it in between.
pub fn apply_downloaded(path: &str, sha256: &str) -> anyhow::Result<()> {
    let bytes = std::fs::read(path)?;
    verify(&bytes, Some(sha256))?;
    replace_with(&bytes)
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
pub fn elevated_update(file: &std::path::Path, sha256: &str) -> anyhow::Result<()> {
    use anyhow::{bail, Context};
    let exe = std::env::current_exe().context("locating current executable")?;
    // Start-Process -Verb RunAs raises the UAC prompt; -Wait -PassThru lets us read the helper's
    // exit code back out. A declined prompt throws, so PowerShell exits non-zero.
    let script = format!(
        "$ErrorActionPreference='Stop';\
         $p=Start-Process -FilePath '{exe}' -ArgumentList '{flag}','\"{file}\"','{sha}' -Verb RunAs -Wait -PassThru;\
         exit $p.ExitCode",
        exe = exe.display(),
        flag = UPDATE_FLAG,
        file = file.display(),
        sha = sha256,
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
pub fn elevated_update(_file: &std::path::Path, _sha256: &str) -> anyhow::Result<()> {
    anyhow::bail!("elevated update is Windows-only")
}

pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Where the newest release is mirrored, for those GitHub serves slowly: `<os>/current` and the
/// sign-in helper, each with a `.json` beside it naming its version and checksum.
pub const MIRROR: &str = "https://eve-spai.com";

/// One file of a release.
#[derive(Clone, Debug)]
pub struct Asset {
    pub api_url: String,
    /// GitHub's checksum of it: whichever source it comes from, this is what it must match.
    pub sha256: Option<String>,
    pub size: u64,
    /// Its path on the mirror.
    pub mirror: &'static str,
}

#[derive(Clone)]
pub struct Available {
    pub version: String,
    pub html_url: String,
    pub asset: Option<Asset>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Github,
    Mirror,
}

/// A download under way.
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub source: Source,
    pub received: u64,
    pub total: u64,
    /// Bytes per second so far.
    pub rate: f64,
}

/// Where the user would rather download from. The mirror is only used once they agreed to it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Prefs {
    pub mirror_ok: bool,
    pub prefer_mirror: bool,
}

#[derive(Clone, Default)]
pub struct UpdateState {
    pub available: Option<Available>,
    pub installing: bool,
    pub done: bool,
    pub error: Option<String>,
    pub progress: Option<Progress>,
    /// The window put away while downloading or once done: the status bar carries it instead.
    pub hidden: bool,
    /// GitHub turned out slow, at this many bytes per second, and the user has not said yet whether
    /// to switch to the mirror.
    pub slow: Option<f64>,
    /// Their answer: switch to the mirror, or keep going.
    pub use_mirror: Option<bool>,
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
    crate::http::client(20).ok()
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
        let asset = asset_of(&json, asset_name(), mirror_path());
        state.lock().unwrap().available = Some(Available { version: tag, html_url, asset });
        finish(&state);
    });
}

fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.').map(|p| p.split('-').next().unwrap_or("").parse::<u64>().unwrap_or(0)).collect()
    };
    parse(a) > parse(b)
}

/// This platform's directory on the mirror.
fn mirror_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows/current"
    } else if cfg!(target_os = "macos") {
        "macos/current"
    } else {
        "linux/current"
    }
}

/// The mirror's page for this platform's download, for a user who prefers it.
pub fn mirror_download_url() -> String {
    format!("{MIRROR}/{}", mirror_path())
}

fn asset_of(release: &serde_json::Value, name: &str, mirror: &'static str) -> Option<Asset> {
    let a = release["assets"].as_array()?.iter().find(|x| x["name"].as_str() == Some(name))?;
    Some(Asset {
        api_url: a["url"].as_str()?.to_owned(),
        sha256: a["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_lowercase),
        size: a["size"].as_u64().unwrap_or(0),
        mirror,
    })
}

/// How long GitHub gets before its speed is judged.
const SPEED_PROBE: Duration = Duration::from_secs(6);

/// Slow enough to be worth the mirror: more than a minute still to go at this rate, and not quick in
/// itself.
pub fn is_slow(rate: f64, received: u64, total: u64) -> bool {
    if rate <= 0.0 || total <= received {
        return false;
    }
    let eta = (total - received) as f64 / rate;
    eta > 60.0 && rate < 2.0 * 1024.0 * 1024.0
}

/// The download, or word that the user would rather switch to the mirror.
enum Got {
    Bytes(Vec<u8>),
    Switch,
}

/// Streams `url`, reporting progress to `state`. From GitHub with `watch`, a slow start asks the
/// user about the mirror (or switches straight away once they agreed to it before).
fn fetch(url: &str, source: Source, size_hint: u64, watch: bool, prefs: Prefs, state: Option<&SharedUpdate>, ctx: Option<&egui::Context>) -> anyhow::Result<Got> {
    use std::io::Read as _;
    // Per read, not for the whole download: a slow line is not a dead one.
    let client = crate::http::client(30)?;
    let mut req = client.get(url);
    if source == Source::Github {
        req = req.header("Accept", "application/octet-stream");
        if let Some(t) = token() {
            req = req.header("Authorization", format!("token {t}"));
        }
    }
    let mut resp = req.send()?.error_for_status()?;
    let total = resp.content_length().unwrap_or(size_hint);
    let mut out = Vec::with_capacity(total as usize);
    let mut chunk = vec![0u8; 64 * 1024];
    let start = std::time::Instant::now();
    let mut shown = start;
    let mut watching = watch;
    loop {
        let n = resp.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..n]);
        let rate = out.len() as f64 / start.elapsed().as_secs_f64().max(0.001);
        if let Some(state) = state {
            if shown.elapsed() >= Duration::from_millis(200) {
                shown = std::time::Instant::now();
                state.lock().unwrap().progress = Some(Progress { source, received: out.len() as u64, total, rate });
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            }
            if watching && start.elapsed() >= SPEED_PROBE && is_slow(rate, out.len() as u64, total) {
                if prefs.mirror_ok {
                    return Ok(Got::Switch);
                }
                let mut s = state.lock().unwrap();
                match s.use_mirror {
                    Some(true) => return Ok(Got::Switch),
                    Some(false) => watching = false,
                    None if s.slow.is_none() => {
                        s.slow = Some(rate);
                        // The question needs the window, even if it was put away.
                        s.hidden = false;
                        if let Some(ctx) = ctx {
                            ctx.request_repaint();
                        }
                    }
                    None => {}
                }
            }
        }
    }
    Ok(Got::Bytes(out))
}

/// The bytes are what GitHub says the file is.
fn verify(bytes: &[u8], sha256: Option<&str>) -> anyhow::Result<()> {
    use sha2::Digest as _;
    let Some(want) = sha256 else { return Ok(()) };
    let got: String = sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    if !got.eq_ignore_ascii_case(want) {
        anyhow::bail!("the download is damaged (checksum mismatch)");
    }
    Ok(())
}

/// From the mirror, only when it carries exactly this version, and only matching GitHub's checksum.
fn from_mirror(asset: &Asset, version: &str, prefs: Prefs, state: Option<&SharedUpdate>, ctx: Option<&egui::Context>) -> anyhow::Result<Vec<u8>> {
    let client = crate::http::client(20)?;
    let meta: serde_json::Value = client.get(format!("{MIRROR}/{}.json", asset.mirror)).send()?.error_for_status()?.json()?;
    if meta["version"].as_str().map(|v| v.trim_start_matches('v')) != Some(version) {
        anyhow::bail!("the mirror does not have v{version} yet");
    }
    let want = asset.sha256.clone().or_else(|| meta["sha256"].as_str().map(str::to_lowercase));
    let Some(want) = want else { anyhow::bail!("no checksum to check the mirror's copy against") };
    let Got::Bytes(bytes) = fetch(&format!("{MIRROR}/{}", asset.mirror), Source::Mirror, asset.size, false, prefs, state, ctx)? else {
        unreachable!("only GitHub downloads are watched")
    };
    verify(&bytes, Some(&want))?;
    Ok(bytes)
}

/// One release file, from wherever the user's choices and GitHub's speed say.
fn get_asset(asset: &Asset, version: &str, prefs: Prefs, state: Option<&SharedUpdate>, ctx: Option<&egui::Context>) -> anyhow::Result<Vec<u8>> {
    if prefs.prefer_mirror {
        match from_mirror(asset, version, prefs, state, ctx) {
            Ok(b) => return Ok(b),
            Err(e) => eprintln!("[update] mirror: {e:#}, trying GitHub"),
        }
    }
    let github = |watch: bool| -> anyhow::Result<Got> { fetch(&asset.api_url, Source::Github, asset.size, watch, prefs, state, ctx) };
    match github(!prefs.prefer_mirror) {
        Ok(Got::Bytes(b)) => {
            verify(&b, asset.sha256.as_deref())?;
            Ok(b)
        }
        Ok(Got::Switch) => match from_mirror(asset, version, prefs, state, ctx) {
            Ok(b) => Ok(b),
            Err(e) => {
                eprintln!("[update] mirror: {e:#}, back to GitHub");
                let Got::Bytes(b) = github(false)? else { unreachable!("not watched") };
                verify(&b, asset.sha256.as_deref())?;
                Ok(b)
            }
        },
        Err(e) if prefs.mirror_ok && !prefs.prefer_mirror => from_mirror(asset, version, prefs, state, ctx).map_err(|m| e.context(format!("the mirror failed too: {m:#}"))),
        Err(e) => Err(e),
    }
}

/// Downloads the release and puts it in place of this binary.
pub fn install(av: &Available, prefs: Prefs, state: &SharedUpdate, ctx: &egui::Context) -> anyhow::Result<()> {
    let asset = av.asset.as_ref().ok_or_else(|| anyhow::anyhow!("this release has no download for this platform"))?;
    let bytes = get_asset(asset, &av.version, prefs, Some(state), Some(ctx))?;
    if update_needs_admin() {
        // Verified here, swapped by an elevated copy of the app that checks it once more.
        let sha = {
            use sha2::Digest as _;
            sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        let file = std::env::temp_dir().join(format!("eve-spai-update-{}.exe", av.version));
        std::fs::write(&file, &bytes)?;
        let res = elevated_update(&file, &sha);
        let _ = std::fs::remove_file(&file);
        res?;
    } else {
        replace_with(&bytes)?;
    }
    Ok(())
}

/// Puts `bytes` in place of this binary.
fn replace_with(bytes: &[u8]) -> anyhow::Result<()> {
    use anyhow::Context;

    let exe = std::env::current_exe().context("locating current executable")?;
    let new_path = exe.with_extension("new");
    // The swap renames twice on Windows, and a failure between them leaves no executable at all
    // with no recovery path. Check for room for the payload, the old binary and the new one
    // before starting, rather than discovering it half way.
    if let Some(dir) = exe.parent() {
        if let Ok(free) = fs4::available_space(dir) {
            let need = (bytes.len() as u64).saturating_mul(3);
            if free < need {
                anyhow::bail!(
                    "not enough free disk space to install the update ({} free, {} needed)",
                    crate::app::fmt_bytes(free),
                    crate::app::fmt_bytes(need)
                );
            }
        }
    }
    std::fs::write(&new_path, bytes).context("writing new binary")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_is_slow_when_the_rest_would_take_over_a_minute() {
        let mb = 1024.0 * 1024.0;
        assert!(is_slow(200.0 * 1024.0, 2_000_000, 42_000_000), "200 KB/s for 40 MB more: minutes");
        assert!(!is_slow(5.0 * mb, 2_000_000, 42_000_000), "fast");
        assert!(!is_slow(300.0 * 1024.0, 40_000_000, 42_000_000), "nearly done anyway");
        assert!(!is_slow(0.0, 0, 42_000_000) && !is_slow(100.0, 10, 0), "nothing to judge yet");
    }

    #[test]
    fn a_download_must_match_githubs_checksum() {
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(verify(b"", Some(empty)).is_ok());
        assert!(verify(b"", Some(&empty.to_uppercase())).is_ok());
        assert!(verify(b"tampered", Some(empty)).is_err());
        assert!(verify(b"anything", None).is_ok(), "an old release without a digest is taken as it comes");
    }

    #[test]
    fn a_release_asset_carries_its_checksum_and_mirror_path() {
        let release = serde_json::json!({"assets": [
            {"name": "eve-spai-linux-x86_64", "url": "https://api.github.com/x/1", "size": 42, "digest": "sha256:ABCDEF"},
            {"name": "eve-spai-windows-x86_64.exe", "url": "https://api.github.com/x/2", "size": 7}
        ]});
        let a = asset_of(&release, "eve-spai-linux-x86_64", "linux/current").unwrap();
        assert_eq!((a.api_url.as_str(), a.sha256.as_deref(), a.size, a.mirror), ("https://api.github.com/x/1", Some("abcdef"), 42, "linux/current"));
        assert_eq!(asset_of(&release, "eve-spai-windows-x86_64.exe", "windows/current").unwrap().sha256, None);
        assert!(asset_of(&release, "missing", "x").is_none());
    }
}
