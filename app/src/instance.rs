use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const CONTROL_PORT: u16 = 52389;

pub const RAISE_BYTE: u8 = b'R';

pub fn control_addr() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, CONTROL_PORT))
}

static RAISE_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn take_raise_request() -> bool {
    RAISE_REQUESTED.swap(false, Ordering::AcqRel)
}

pub fn should_raise(first_byte: Option<u8>) -> bool {
    matches!(first_byte, None | Some(RAISE_BYTE))
}

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
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let mut buf = [0u8; 1];
            let first = match stream.read(&mut buf) {
                Ok(0) => None,
                Ok(_) => Some(buf[0]),
                Err(_) => None,
            };
            if should_raise(first) {
                RAISE_REQUESTED.store(true, Ordering::Release);
                ctx.request_repaint();
            }
        }
    });
}

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
        assert!(should_raise(None));
        assert!(should_raise(Some(RAISE_BYTE)));
        assert!(!should_raise(Some(b'X')));
        assert!(!should_raise(Some(0)));
    }
}
