//! Signing in to the dashboard, in a browser engine we drive.
//!
//! `fleets.gnf.lt` accepts one credential: its own session cookie, minted by an OIDC round trip
//! through GICE that lands on the site's own origin. No loopback redirect to catch, and the cookie
//! is non-persistent so it never reaches any browser's cookie file. The only way to hold it is to
//! run the login ourselves and read the jar.
//!
//! It runs in a subprocess on every platform. The webview's event loop and `eframe::run_native`
//! both want the main thread, and macOS will only run an event loop there. On Linux there are two
//! more reasons: wry has no `raw-window-handle` path and needs a real GTK container, and GTK needs
//! `GDK_BACKEND` set process-wide to match the X11 forcing `main.rs` already does for winit. A
//! crashed engine staying out of the app is a bonus.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// The argument that turns this binary into the login child.
pub const FLAG: &str = "--fleet-login";

/// The child speaks, the parent listens. Deliberately not `OverlayToMain`: that enum crosses the
/// web-page relay, and a session cookie has no business on it.
#[derive(Serialize, Deserialize, Debug)]
pub enum LoginToMain {
    /// The window is up. Only so the parent can tell "still signing in" from "never started".
    Hello,
    /// A cookie jar the child already proved works, as `name=value; name=value`.
    Ok { cookies: String },
    Failed { why: String },
    Cancelled,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub enum LoginStatus {
    #[default]
    Idle,
    /// The window is open and the user is typing into it.
    Waiting,
    Done(String),
    Failed(String),
}

pub type SharedLogin = Arc<Mutex<LoginStatus>>;

pub fn status(shared: &SharedLogin) -> LoginStatus {
    shared.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub(crate) fn set(shared: &SharedLogin, s: LoginStatus) {
    *shared.lock().unwrap_or_else(|e| e.into_inner()) = s;
}

#[cfg(all(
    feature = "fleet-auth",
    any(target_os = "linux", target_os = "windows", target_os = "macos")
))]
mod webview;
#[cfg(not(all(
    feature = "fleet-auth",
    any(target_os = "linux", target_os = "windows", target_os = "macos")
)))]
mod other;

#[cfg(all(
    feature = "fleet-auth",
    any(target_os = "linux", target_os = "windows", target_os = "macos")
))]
pub use webview::{run_child, spawn_login};
#[cfg(not(all(
    feature = "fleet-auth",
    any(target_os = "linux", target_os = "windows", target_os = "macos")
)))]
pub use other::{run_child, spawn_login};

/// How long a sign-in window may stay open.
const WAIT: std::time::Duration = std::time::Duration::from_secs(300);

/// Runs `exe --fleet-login` and waits for what it reports. The child is this binary in builds
/// with the webview, and the `eve-spai-fleet-login` helper next to it in builds without.
pub(crate) fn run_login_process(exe: &std::path::Path) -> LoginStatus {
    use std::io::Read as _;
    use std::process::{Command, Stdio};
    let mut child = match Command::new(exe).arg(FLAG).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => return LoginStatus::Failed(format!("cannot start the sign-in window: {e}")),
    };
    let Some(mut out) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return LoginStatus::Failed("the sign-in window has no output pipe".to_owned());
    };
    // Kept for a child that dies before it says anything: the loader's reason is the useful part.
    let err = child.stderr.take().map(|mut e| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = e.read_to_string(&mut text);
            eprint!("{text}");
            text
        })
    });

    // Read on a thread so a child that hangs on webkit still hits the deadline.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || loop {
        match crate::ipc::recv::<LoginToMain, _>(&mut out) {
            Ok(LoginToMain::Hello) => continue,
            Ok(msg) => {
                let _ = tx.send(Some(msg));
                return;
            }
            Err(_) => {
                let _ = tx.send(None);
                return;
            }
        }
    });
    let got = rx.recv_timeout(WAIT);
    // Unconditionally, so a cancelled or timed-out login leaves no webkit process behind.
    let _ = child.kill();
    let _ = child.wait();
    let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();

    match got {
        Ok(Some(LoginToMain::Ok { cookies })) => match crate::fleets::creds::save(&cookies) {
            Ok(()) => LoginStatus::Done(cookies),
            Err(e) => LoginStatus::Failed(format!("signed in, but the keychain refused: {e:#}")),
        },
        Ok(Some(LoginToMain::Failed { why })) => LoginStatus::Failed(why),
        Ok(None) if stderr.contains("error while loading shared libraries") => {
            LoginStatus::Failed(missing_library_message(&stderr))
        }
        Ok(Some(LoginToMain::Cancelled)) | Ok(None) | Ok(Some(LoginToMain::Hello)) => LoginStatus::Idle,
        Err(_) => LoginStatus::Failed("the sign-in window timed out".to_owned()),
    }
}

/// What to install, for a helper the dynamic loader refused to start.
fn missing_library_message(stderr: &str) -> String {
    let lib = stderr
        .split("error while loading shared libraries:")
        .nth(1)
        .and_then(|r| r.split(':').next())
        .map(str::trim)
        .unwrap_or("a system library");
    format!("Signing in needs {lib}, which is not installed. {}", WEBKIT_HOW)
}

/// Where the Linux sign-in helper is: next to this binary.
pub const HELPER: &str = crate::update::HELPER_NAME;

const WEBKIT_HOW: &str = "Install WebKitGTK 4.1: Fedora `sudo dnf install webkit2gtk4.1`, Bazzite and other \
    atomic Fedoras `rpm-ostree install webkit2gtk4.1` (then reboot), Debian and Ubuntu \
    `sudo apt install libwebkit2gtk-4.1-0`, Arch `sudo pacman -S webkit2gtk-4.1`.";

/// On Linux builds that sign in through the helper, whether WebKitGTK 4.1 looks absent, and what
/// to tell the user. A path check, so it can be shown before anyone tries to sign in.
pub fn webkit_missing() -> Option<String> {
    if !cfg!(target_os = "linux") || cfg!(feature = "fleet-auth") {
        return None;
    }
    const DIRS: [&str; 7] = [
        "/usr/lib64",
        "/usr/lib",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/lib64",
        "/lib/x86_64-linux-gnu",
        "/usr/local/lib",
    ];
    let found = DIRS.iter().any(|d| std::path::Path::new(d).join("libwebkit2gtk-4.1.so.0").exists());
    (!found).then(|| format!("Signing in to the fleet dashboard needs WebKitGTK 4.1, which was not found. {WEBKIT_HOW}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loader_failure_names_the_missing_library() {
        let err = "eve-spai-fleet-login: error while loading shared libraries: libwebkit2gtk-4.1.so.0: cannot open shared object file";
        let m = missing_library_message(err);
        assert!(m.starts_with("Signing in needs libwebkit2gtk-4.1.so.0"), "{m}");
        assert!(m.contains("rpm-ostree"));
    }
}
