//! Every build without the webview: the same surface, answering that it cannot.
//!
//! Callers never `cfg`, which is what keeps `--all-features` on the Windows and macOS runners
//! honest about the shape of the Linux side.

use super::{set, LoginStatus, SharedLogin};

pub fn spawn_login(shared: SharedLogin, ctx: egui::Context) {
    // Two different reasons, and only one of them is fixed by rebuilding.
    let why = if cfg!(any(target_os = "linux", target_os = "windows", target_os = "macos")) {
        "this build has no embedded browser: rebuild with --features fleet-auth"
    } else {
        "signing in needs an embedded browser, and there is none for this platform"
    };
    set(&shared, LoginStatus::Failed(why.to_owned()));
    ctx.request_repaint();
}

pub fn run_child() -> eframe::Result<()> {
    eprintln!("[fleet-login] this build has no embedded browser");
    Ok(())
}
