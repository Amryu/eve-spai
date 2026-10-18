//! Hands a URL or path to the desktop. Tests click the buttons that do this, and the desktop must
//! not get their fixtures.

pub fn that<T: AsRef<std::ffi::OsStr>>(target: T) -> std::io::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    open::that(target)
}
