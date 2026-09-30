//! Makes clicked `eve-spai://` links (group invites) open this app: a desktop entry on Linux, the
//! per-user registry on Windows. macOS releases are a bare binary with no bundle to declare the
//! scheme in, so there the web page's fallback (paste the link) is the way.

/// Registers this binary for `eve-spai://`, when it is not already. Runs off the UI thread.
pub fn register() {
    let Ok(exe) = std::env::current_exe() else { return };
    // A build under `target/` is a developer's, not the installed app: it must not take the links.
    if exe.components().any(|c| c.as_os_str() == "target") {
        return;
    }
    if let Err(e) = register_exe(&exe) {
        eprintln!("[links] eve-spai:// not registered: {e}");
    }
}

#[cfg(target_os = "linux")]
fn register_exe(exe: &std::path::Path) -> std::io::Result<()> {
    const NAME: &str = "eve-spai-links.desktop";
    let dir = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/share")))
        .ok_or_else(|| std::io::Error::other("no home directory"))?
        .join("applications");
    let entry = entry(exe);
    let path = dir.join(NAME);
    let fresh = std::fs::read_to_string(&path).map_or(true, |old| old != entry);
    if fresh {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&path, &entry)?;
        let _ = std::process::Command::new("update-desktop-database").arg(&dir).output();
    }
    let current = std::process::Command::new("xdg-mime").args(["query", "default", "x-scheme-handler/eve-spai"]).output()?;
    if fresh || String::from_utf8_lossy(&current.stdout).trim() != NAME {
        std::process::Command::new("xdg-mime").args(["default", NAME, "x-scheme-handler/eve-spai"]).output()?;
    }
    Ok(())
}

/// A hidden entry: it only tells the desktop which program opens the links.
#[cfg(target_os = "linux")]
fn entry(exe: &std::path::Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=EVE Spai\nComment=Opens EVE Spai group invites\nExec=\"{}\" %u\nNoDisplay=true\nTerminal=false\nMimeType=x-scheme-handler/eve-spai;\n",
        exe.display()
    )
}

#[cfg(target_os = "windows")]
fn register_exe(exe: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt as _;
    // No console window flashing up for each `reg` call.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let reg = |args: &[&str]| std::process::Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output();
    let command = format!("\"{}\" \"%1\"", exe.display());
    let key = r"HKCU\Software\Classes\eve-spai";
    let open = format!(r"{key}\shell\open\command");
    let now = reg(&["query", &open, "/ve"])?;
    if String::from_utf8_lossy(&now.stdout).contains(&command) {
        return Ok(());
    }
    reg(&["add", key, "/ve", "/d", "URL:EVE Spai", "/f"])?;
    reg(&["add", key, "/v", "URL Protocol", "/d", "", "/f"])?;
    reg(&["add", &open, "/ve", "/d", &command, "/f"])?;
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn register_exe(_: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn the_entry_hands_the_link_to_this_binary() {
        let e = super::entry(std::path::Path::new("/home/p/.local/bin/eve-spai"));
        assert!(e.contains("Exec=\"/home/p/.local/bin/eve-spai\" %u"), "{e}");
        assert!(e.contains("MimeType=x-scheme-handler/eve-spai;") && e.contains("NoDisplay=true"));
    }
}
