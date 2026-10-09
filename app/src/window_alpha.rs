//! Fading a whole window, for the map overlay on Windows. A per-pixel transparent DX12 swapchain
//! there crashed the GPU driver when the window crossed monitors, so the overlay window is opaque
//! and Windows fades it as a layered window instead. Elsewhere the overlay draws its own
//! see-through backdrop and this does nothing.

/// Sets the window titled `title` to `alpha` (0 to 1). Cheap to call every frame: it only acts when
/// the value changes.
#[cfg(windows)]
pub fn set(title: &str, alpha: f32) {
    use std::sync::Mutex;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED,
    };
    static LAST: Mutex<Option<(String, u8)>> = Mutex::new(None);
    let a = (alpha.clamp(0.2, 1.0) * 255.0) as u8;
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_ref().is_some_and(|(t, v)| t == title && *v == a) {
        return;
    }
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: a NUL-terminated UTF-16 title, and a window handle checked for null before use.
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd.is_null() {
            return;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as isize);
        SetLayeredWindowAttributes(hwnd, 0, a, LWA_ALPHA);
    }
    *last = Some((title.to_owned(), a));
}

#[cfg(not(windows))]
pub fn set(_title: &str, _alpha: f32) {}

/// Whether the overlay draws its own see-through backdrop, or is faded as a whole by [`set`].
pub const PER_PIXEL: bool = !cfg!(windows);
