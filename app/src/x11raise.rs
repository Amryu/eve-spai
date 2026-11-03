//! Raising one of this app's windows on X11 without taking the focus.
//!
//! KWin keeps a window's place among normal windows while it is kept above, so setting keep-above
//! for a frame and clearing it again, the way Windows is made to raise a window, drops the window
//! straight back behind whatever covered it. A raise asked for as a pager (`_NET_RESTACK_WINDOW`,
//! source 2) is honoured as it stands, focus-stealing prevention or not, and leaves the focus where
//! it is, on the game.

/// Raises every window of this process titled `title` above the other normal windows. Off the
/// caller's thread, since it waits on the X server.
#[cfg(target_os = "linux")]
pub fn raise(title: &str) {
    let title = title.to_owned();
    let _ = std::thread::Builder::new().name("x11-raise".into()).spawn(move || {
        if let Err(e) = raise_now(&title) {
            eprintln!("[x11] could not raise {title:?}: {e}");
        }
    });
}

#[cfg(not(target_os = "linux"))]
pub fn raise(_title: &str) {}

#[cfg(target_os = "linux")]
fn raise_now(title: &str) -> anyhow::Result<()> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask};

    let (conn, screen) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen].root;
    let atom = |name: &str| -> anyhow::Result<u32> { Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom) };
    let (list, pid, name, utf8, restack) =
        (atom("_NET_CLIENT_LIST")?, atom("_NET_WM_PID")?, atom("_NET_WM_NAME")?, atom("UTF8_STRING")?, atom("_NET_RESTACK_WINDOW")?);
    let clients = conn.get_property(false, root, list, AtomEnum::WINDOW, 0, u32::MAX)?.reply()?;
    let me = std::process::id();
    for w in clients.value32().into_iter().flatten() {
        let owner = conn.get_property(false, w, pid, AtomEnum::CARDINAL, 0, 1)?.reply()?;
        if owner.value32().and_then(|mut v| v.next()) != Some(me) {
            continue;
        }
        let t = conn.get_property(false, w, name, utf8, 0, 1024)?.reply()?;
        if String::from_utf8_lossy(&t.value) != title {
            continue;
        }
        // Source 2 is a pager; no sibling and Above put it on top of its layer.
        let ev = ClientMessageEvent::new(32, w, restack, [2, 0, 0, 0, 0]);
        conn.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev)?;
    }
    conn.flush()?;
    Ok(())
}
