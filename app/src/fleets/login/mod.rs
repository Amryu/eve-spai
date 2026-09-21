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
