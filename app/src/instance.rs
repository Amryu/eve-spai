//! Second-launch raises the running instance.
//!
//! The single-instance file lock (main.rs) decides who the primary is. On top of it, the
//! primary listens on a fixed loopback TCP port; when a second copy is launched it finds the
//! lock held, connects to that port, sends a one-byte "raise" request, and exits without ever
//! opening a window. The primary's listener thread flips a flag and wakes the UI, which issues
//! the portable eframe viewport commands to de-minimize + focus + raise the main window.
//!
//! TCP on 127.0.0.1 is the portable IPC here: it works identically on Linux, Windows, and
//! macOS with no per-OS window APIs. Every failure mode (port already held, primary not
//! listening yet, connection refused) degrades gracefully to today's behaviour.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Fixed loopback control port the primary listens on. Chosen in the IANA dynamic/private
/// range (49152-65535) to avoid well-known services; a collision just disables the feature.
pub const CONTROL_PORT: u16 = 52389;

/// One-byte message a secondary sends to ask the primary to raise its window.
pub const RAISE_BYTE: u8 = b'R';

/// Loopback control address (127.0.0.1:CONTROL_PORT), used for both bind and connect.
pub fn control_addr() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, CONTROL_PORT))
}

/// Set by the listener thread when a secondary asks us to raise; consumed by the UI loop.
static RAISE_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Consume a pending raise request. Returns true at most once per request; the UI loop calls
/// this each frame and, when true, issues the raise viewport commands.
pub fn take_raise_request() -> bool {
    RAISE_REQUESTED.swap(false, Ordering::AcqRel)
}

/// Decide whether a control connection should raise the window, given the first byte read
/// (or `None` when the peer connected and closed without sending anything). A bare connection
/// and the explicit RAISE_BYTE both raise; any other byte is ignored so stray traffic on the
/// port cannot pop the window.
pub fn should_raise(first_byte: Option<u8>) -> bool {
    matches!(first_byte, None | Some(RAISE_BYTE))
}

/// Start the primary's control listener. Best-effort and non-blocking: on a bind failure it
/// logs one line and returns, leaving the app fully functional (just without raise-on-relaunch).
/// The accept loop runs on a detached background thread and only ever touches the passed
/// `Context` to request a repaint after flagging a raise.
pub fn start_control_listener(ctx: egui::Context) {
    let listener = match TcpListener::bind(control_addr()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "[instance] control port {CONTROL_PORT} bind failed ({e}); raise-on-second-launch disabled"
            );
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // Read at most one byte with a short timeout so a silent peer cannot wedge the loop.
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let mut buf = [0u8; 1];
            let first = match stream.read(&mut buf) {
                Ok(0) => None,          // peer connected then closed: bare "raise"
                Ok(_) => Some(buf[0]),
                Err(_) => None,         // timed out / reset: treat as a bare raise
            };
            if should_raise(first) {
                RAISE_REQUESTED.store(true, Ordering::Release);
                ctx.request_repaint();
            }
        }
    });
}

/// Secondary path: ask an already-running primary to raise its window. Returns true if the
/// raise byte was delivered (the caller should then exit 0 without opening a window); false if
/// no primary was listening, in which case the caller falls back to a plain quiet exit.
pub fn signal_raise() -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&control_addr(), Duration::from_millis(500))
    else {
        return false;
    };
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    stream.write_all(&[RAISE_BYTE]).and_then(|()| stream.flush()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_addr_is_loopback_on_fixed_port() {
        let addr = control_addr();
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), CONTROL_PORT);
        assert_eq!(addr.to_string(), format!("127.0.0.1:{CONTROL_PORT}"));
    }

    #[test]
    fn bare_connection_and_raise_byte_raise_others_ignored() {
        assert!(should_raise(None)); // connect + close with no data
        assert!(should_raise(Some(RAISE_BYTE)));
        assert!(!should_raise(Some(b'X')));
        assert!(!should_raise(Some(0)));
    }
}
