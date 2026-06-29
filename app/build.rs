// Embed the application icon into the Windows .exe so the taskbar button, Explorer entry, and a
// pinned shortcut all show it. The runtime `with_icon` (see `app::app_icon`) only sets the in-app
// window icon — Windows takes the taskbar/shell icon from the executable's embedded resource.
//
// Gated on the TARGET being Windows (via CARGO_CFG_TARGET_OS, which build scripts can read), not
// the host, so cross-builds behave and non-Windows builds skip it entirely. winresource itself is
// pure Rust and links fine everywhere; only `.compile()` needs a resource compiler (rc.exe on the
// Windows CI runner).
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
        let icon = std::path::Path::new(&manifest).join("../assets/eve-spai.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon.to_str().expect("icon path is valid UTF-8"));
        if let Err(e) = res.compile() {
            // Don't fail the build over the icon — just note it.
            println!("cargo:warning=could not embed the Windows app icon: {e}");
        }
    }
}
