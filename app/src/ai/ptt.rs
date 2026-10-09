//! The push-to-talk key, held anywhere: in the game, on the desktop.
//!
//! Linux listens for XInput2 raw key and button events on the X server, which reach us while the
//! game's (XWayland) window has focus; exclusive mode also grabs the key so the game never sees
//! it. Windows uses a low-level keyboard hook. macOS has no global key here: `eve-spai --ptt` on
//! a system shortcut toggles listening instead, as it does anywhere.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::ai::config::KeyBind;

#[derive(Clone, Debug, PartialEq)]
pub enum PttEvent {
    Down,
    Up,
    /// The next key pressed while binding.
    Bound(KeyBind),
}

/// Mouse buttons share the code space with keys, above any key code.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const BUTTON_BASE: u32 = 0x1_0000;

pub struct Ptt {
    pub bind: Arc<Mutex<Option<KeyBind>>>,
    pub exclusive: Arc<AtomicBool>,
    /// The next key pressed is reported as [`PttEvent::Bound`] instead of acting.
    pub binding: Arc<AtomicBool>,
    pub rx: Receiver<PttEvent>,
    /// Why the listener is not running, when it is not.
    pub problem: Arc<Mutex<Option<String>>>,
}

/// Whether this platform has a global key.
pub const SUPPORTED: bool = cfg!(any(target_os = "linux", target_os = "windows"));

pub fn spawn(ctx: egui::Context) -> Ptt {
    let (tx, rx) = std::sync::mpsc::channel();
    let p = Ptt { bind: Default::default(), exclusive: Default::default(), binding: Default::default(), rx, problem: Default::default() };
    let shared = Shared { bind: p.bind.clone(), exclusive: p.exclusive.clone(), binding: p.binding.clone(), tx, ctx };
    let problem = p.problem.clone();
    let _ = std::thread::Builder::new().name("ai-ptt".into()).spawn(move || {
        if let Err(e) = listen(shared) {
            *problem.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
        }
    });
    p
}

struct Shared {
    bind: Arc<Mutex<Option<KeyBind>>>,
    exclusive: Arc<AtomicBool>,
    binding: Arc<AtomicBool>,
    tx: Sender<PttEvent>,
    ctx: egui::Context,
}

impl Shared {
    fn code(&self) -> Option<u32> {
        self.bind.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|b| b.platform == std::env::consts::OS && b.code != 0).map(|b| b.code)
    }

    /// A key went down or up. Returns whether it was the talk key (to swallow it when exclusive).
    fn key(&self, code: u32, down: bool, label: impl FnOnce() -> String, held: &mut bool) -> bool {
        if self.binding.load(Ordering::Relaxed) {
            if down {
                self.binding.store(false, Ordering::Relaxed);
                let _ = self.tx.send(PttEvent::Bound(KeyBind { code, label: label(), platform: std::env::consts::OS.to_owned() }));
                self.ctx.request_repaint();
            }
            return false;
        }
        if self.code() != Some(code) {
            return false;
        }
        // Held keys repeat; only the first press and the release count.
        if down != *held {
            *held = down;
            let _ = self.tx.send(if down { PttEvent::Down } else { PttEvent::Up });
            self.ctx.request_repaint();
        }
        true
    }
}

#[cfg(target_os = "linux")]
fn listen(s: Shared) -> anyhow::Result<()> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xinput::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::{self, ConnectionExt as _};
    use x11rb::protocol::Event;

    let (conn, screen) = x11rb::connect(None).map_err(|_| anyhow::anyhow!("no X server to listen on: the talk key works while an X11 or XWayland window, like the game, has focus"))?;
    let root = conn.setup().roots[screen].root;
    conn.xinput_xi_query_version(2, 2)?.reply()?;
    let mask = xinput::EventMask {
        deviceid: xinput::Device::ALL_MASTER.into(),
        mask: vec![(xinput::XIEventMask::RAW_KEY_PRESS | xinput::XIEventMask::RAW_KEY_RELEASE | xinput::XIEventMask::RAW_BUTTON_PRESS | xinput::XIEventMask::RAW_BUTTON_RELEASE).into()],
    };
    conn.xinput_xi_select_events(root, &[mask])?.check()?;
    conn.flush()?;
    let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
    let map = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
    let label = |code: u32| -> String {
        if code >= BUTTON_BASE {
            return format!("Mouse button {}", code - BUTTON_BASE);
        }
        let per = map.keysyms_per_keycode as usize;
        let i = (code as usize).saturating_sub(min as usize) * per;
        map.keysyms.get(i).map_or_else(|| format!("Key {code}"), |&k| keysym_name(k, code))
    };
    let mut held = false;
    let mut grabbed: Option<u32> = None;
    loop {
        // Grab or release the key when the binding or the mode changed.
        let want = s.code().filter(|c| *c < BUTTON_BASE && s.exclusive.load(Ordering::Relaxed));
        if want != grabbed {
            if let Some(c) = grabbed.take() {
                let _ = conn.ungrab_key(c as u8, root, xproto::ModMask::ANY);
            }
            if let Some(c) = want {
                if conn.grab_key(false, root, xproto::ModMask::ANY, c as u8, xproto::GrabMode::ASYNC, xproto::GrabMode::ASYNC)?.check().is_ok() {
                    grabbed = Some(c);
                }
            }
            conn.flush()?;
        }
        while let Some(ev) = conn.poll_for_event()? {
            match ev {
                Event::XinputRawKeyPress(e) => {
                    s.key(e.detail, true, || label(e.detail), &mut held);
                }
                Event::XinputRawKeyRelease(e) => {
                    s.key(e.detail, false, || label(e.detail), &mut held);
                }
                // Buttons 1 to 3 and the wheel are never a talk key.
                Event::XinputRawButtonPress(e) if e.detail >= 8 => {
                    s.key(BUTTON_BASE + e.detail, true, || label(BUTTON_BASE + e.detail), &mut held);
                }
                Event::XinputRawButtonRelease(e) if e.detail >= 8 => {
                    s.key(BUTTON_BASE + e.detail, false, || label(BUTTON_BASE + e.detail), &mut held);
                }
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// A key's name from its keysym, for the settings to show.
#[cfg(target_os = "linux")]
fn keysym_name(k: u32, code: u32) -> String {
    match k {
        0x20..=0x7e => (char::from_u32(k).unwrap_or('?')).to_ascii_uppercase().to_string(),
        0xffbe..=0xffd5 => format!("F{}", k - 0xffbe + 1),
        0xffe1 => "Left Shift".into(),
        0xffe2 => "Right Shift".into(),
        0xffe3 => "Left Ctrl".into(),
        0xffe4 => "Right Ctrl".into(),
        0xffe9 | 0xffe7 => "Left Alt".into(),
        0xffea | 0xfe03 => "Right Alt".into(),
        0xffe5 => "Caps Lock".into(),
        0xff09 => "Tab".into(),
        0xff13 => "Pause".into(),
        0xff14 => "Scroll Lock".into(),
        0xff61 => "Print".into(),
        0xff63 => "Insert".into(),
        0xff50 => "Home".into(),
        0xff57 => "End".into(),
        0xff55 => "Page Up".into(),
        0xff56 => "Page Down".into(),
        0xffff => "Delete".into(),
        0xffb0..=0xffb9 => format!("Num {}", k - 0xffb0),
        _ => format!("Key {code}"),
    }
}

#[cfg(target_os = "windows")]
fn listen(s: Shared) -> anyhow::Result<()> {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    static SHARED: OnceLock<Mutex<(Shared, bool)>> = OnceLock::new();
    if SHARED.set(Mutex::new((s, false))).is_err() {
        anyhow::bail!("the talk key listener is already running");
    }

    unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            let k = &*(l as *const KBDLLHOOKSTRUCT);
            let down = matches!(w as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let up = matches!(w as u32, WM_KEYUP | WM_SYSKEYUP);
            if down || up {
                if let Some(m) = SHARED.get() {
                    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
                    let (sh, held) = &mut *g;
                    let ours = sh.key(k.vkCode, down, || vk_name(k.vkCode), held);
                    if ours && sh.exclusive.load(Ordering::Relaxed) {
                        return 1;
                    }
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, w, l)
    }

    unsafe {
        let h = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), GetModuleHandleW(std::ptr::null()), 0);
        if h.is_null() {
            anyhow::bail!("Windows refused the keyboard hook");
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn vk_name(vk: u32) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(vk).map(String::from).unwrap_or_default(),
        0x70..=0x87 => format!("F{}", vk - 0x6f),
        0x05 => "Mouse 4".into(),
        0x06 => "Mouse 5".into(),
        0x10 | 0xa0 => "Left Shift".into(),
        0xa1 => "Right Shift".into(),
        0x11 | 0xa2 => "Left Ctrl".into(),
        0xa3 => "Right Ctrl".into(),
        0x12 | 0xa4 => "Left Alt".into(),
        0xa5 => "Right Alt".into(),
        0x14 => "Caps Lock".into(),
        0x09 => "Tab".into(),
        0x13 => "Pause".into(),
        0x91 => "Scroll Lock".into(),
        0x2d => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "Page Up".into(),
        0x22 => "Page Down".into(),
        0x2e => "Delete".into(),
        0xc0 => "`".into(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        _ => format!("Key {vk}"),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn listen(_s: Shared) -> anyhow::Result<()> {
    anyhow::bail!("no global talk key on this system: bind `eve-spai --ptt` to a keyboard shortcut instead")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> (Shared, Receiver<PttEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let bind = Arc::new(Mutex::new(Some(KeyBind { code: 118, label: "F9".into(), platform: std::env::consts::OS.into() })));
        (Shared { bind, exclusive: Default::default(), binding: Default::default(), tx, ctx: egui::Context::default() }, rx)
    }

    /// Starts the real listener on this machine's X server: `cargo test --bin eve-spai ptt_live -- --ignored`.
    #[test]
    #[ignore = "needs an X server"]
    fn ptt_live() {
        let p = spawn(egui::Context::default());
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert_eq!(*p.problem.lock().unwrap(), None);
    }

    #[test]
    fn the_talk_key_reports_once_down_and_once_up_and_binding_takes_the_next_key() {
        let (s, rx) = shared();
        let mut held = false;
        assert!(s.key(118, true, || "F9".into(), &mut held));
        assert!(s.key(118, true, || "F9".into(), &mut held), "a repeat is still ours, but says nothing");
        assert!(!s.key(40, true, || "A".into(), &mut held));
        assert!(s.key(118, false, || "F9".into(), &mut held));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![PttEvent::Down, PttEvent::Up]);
        s.binding.store(true, Ordering::Relaxed);
        assert!(!s.key(40, true, || "A".into(), &mut held));
        assert!(matches!(rx.try_recv(), Ok(PttEvent::Bound(KeyBind { code: 40, .. }))));
        assert!(!s.binding.load(Ordering::Relaxed));
        *s.bind.lock().unwrap() = Some(KeyBind { code: 118, label: "F9".into(), platform: "plan9".into() });
        assert!(!s.key(118, true, || "F9".into(), &mut held), "a binding from another system is ignored");
    }
}
