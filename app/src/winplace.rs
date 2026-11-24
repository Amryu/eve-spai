//! Whether a window can still be reached with the mouse, on Windows. A position restored from
//! settings can lie outside every monitor (a display gone, a scale changed), and a window opened
//! there has no title bar to drag back.

/// Whether the window titled `title` has the middle of its title bar on no monitor. False when the
/// window is not found or is minimized.
#[cfg(windows)]
pub fn title_bar_off_screen(title: &str) -> bool {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONULL};
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect, IsIconic};
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: a NUL-terminated UTF-16 title, a window handle checked for null before use, and a
    // RECT that outlives the call filling it.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() || IsIconic(hwnd) != 0 {
            return false;
        }
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        if GetWindowRect(hwnd, &mut r) == 0 {
            return false;
        }
        let grab = POINT { x: (r.left + r.right) / 2, y: r.top + 16 };
        MonitorFromPoint(grab, MONITOR_DEFAULTTONULL).is_null()
    }
}

#[cfg(not(windows))]
pub fn title_bar_off_screen(_title: &str) -> bool {
    false
}
