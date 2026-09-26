//! Every build without the webview: the same surface, answering that it cannot.
//!
//! Callers never `cfg`, which is what keeps `--all-features` on the Windows and macOS runners
//! honest about the shape of the Linux side.

use super::{set, LoginStatus, SharedLogin};

pub fn spawn_login(shared: SharedLogin, ctx: egui::Context) {
    // Linux release builds keep WebKitGTK out of the app itself, so it starts on machines without
    // it; the sign-in window is a helper next to this binary that links it.
    if cfg!(target_os = "linux") {
        set(&shared, LoginStatus::Waiting);
        ctx.request_repaint();
        std::thread::spawn(move || {
            let here = std::env::current_exe().ok().map(|e| e.with_file_name(super::HELPER)).filter(|h| h.exists());
            // An install updated by a version that did not know about the helper has none yet.
            let helper = match here {
                Some(h) => Ok(h),
                None => crate::update::fetch_helper_for_current(),
            };
            let outcome = match helper {
                Ok(h) => super::run_login_process(&h),
                Err(e) => LoginStatus::Failed(format!(
                    "the sign-in helper {} is missing and could not be downloaded ({e:#}); reinstall with install.sh",
                    super::HELPER
                )),
            };
            set(&shared, outcome);
            ctx.request_repaint();
        });
        return;
    }
    // Two different reasons, and only one of them is fixed by rebuilding.
    let why = if cfg!(any(target_os = "windows", target_os = "macos")) {
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
