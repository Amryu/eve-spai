//! Talking to a running Mumble over its session-bus interface.
//!
//! Mumble exposes `net.sourceforge.mumble.Mumble` on the session bus, which answers where the
//! client currently is and can be told to go somewhere else. That is enough to say whether the FC
//! is actually sitting in the channel their own fleet claims, and to put them there.
//!
//! Everything here degrades to `None`: Mumble may not be running, may be a build without D-Bus, or
//! may be on a machine with no session bus at all. None of that is an error worth surfacing.

/// Where Mumble says it is, as a `mumble://` URL.
///
/// The path is percent-encoded and carries the whole channel path, which is what makes it possible
/// to compare against a link without asking Mumble to enumerate anything.
#[cfg(target_os = "linux")]
pub fn current_url() -> Option<String> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "net.sourceforge.mumble.mumble",
        "/",
        "net.sourceforge.mumble.Mumble",
    )
    .ok()?;
    proxy.call::<_, _, String>("getCurrentUrl", &()).ok()
}

/// Sends the client to a channel. Falls back to the desktop handler when Mumble is not on the bus,
/// since a `mumble://` URL opens the client either way.
#[cfg(target_os = "linux")]
pub fn open_url(url: &str) {
    let sent = (|| {
        let conn = zbus::blocking::Connection::session().ok()?;
        let proxy = zbus::blocking::Proxy::new(
            &conn,
            "net.sourceforge.mumble.mumble",
            "/",
            "net.sourceforge.mumble.Mumble",
        )
        .ok()?;
        proxy.call::<_, _, ()>("openUrl", &(url,)).ok()
    })()
    .is_some();
    if !sent {
        let _ = open::that(url);
    }
}

/// Calls a method of Mumble's bus interface; None when Mumble is not there to answer.
#[cfg(target_os = "linux")]
fn call<A, R>(method: &str, args: &A) -> Option<R>
where
    A: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let conn = zbus::blocking::Connection::session().ok()?;
    let proxy = zbus::blocking::Proxy::new(&conn, "net.sourceforge.mumble.mumble", "/", "net.sourceforge.mumble.Mumble").ok()?;
    proxy.call::<_, _, R>(method, args).ok()
}

/// How Mumble stands: muted, deafened, who is talking, how it transmits. None when it is not
/// running or has no bus interface (Windows and macOS builds have none).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub muted: bool,
    pub deaf: bool,
    pub talking: Vec<String>,
    /// 0 continuous, 1 voice activity, 2 push to talk.
    pub transmit: u32,
}

#[cfg(target_os = "linux")]
pub fn status() -> Option<Status> {
    Some(Status {
        muted: call::<_, bool>("isSelfMuted", &())?,
        deaf: call::<_, bool>("isSelfDeaf", &())?,
        talking: call::<_, Vec<String>>("getTalkingUsers", &()).unwrap_or_default(),
        transmit: call::<_, u32>("getTransmitMode", &()).unwrap_or(2),
    })
}

#[cfg(target_os = "linux")]
pub fn set_self_muted(on: bool) -> bool {
    call::<_, ()>("setSelfMuted", &(on,)).is_some()
}

#[cfg(target_os = "linux")]
pub fn set_self_deaf(on: bool) -> bool {
    call::<_, ()>("setSelfDeaf", &(on,)).is_some()
}

#[cfg(target_os = "linux")]
pub fn set_transmit(mode: u32) -> bool {
    call::<_, ()>("setTransmitMode", &(mode,)).is_some()
}

#[cfg(not(target_os = "linux"))]
pub fn status() -> Option<Status> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn set_self_muted(_on: bool) -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
pub fn set_self_deaf(_on: bool) -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
pub fn set_transmit(_mode: u32) -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
pub fn current_url() -> Option<String> {
    None
}

#[cfg(not(target_os = "linux"))]
pub fn open_url(url: &str) {
    let _ = open::that(url);
}

/// Whether Mumble is sitting in the channel a link points at.
///
/// Compared on the channel path alone: the URLs carry a `?title=` and a `?version=` that differ
/// between what the dashboard hands out and what Mumble reports, and the host is spelled several
/// ways for the same server.
pub fn in_channel(current: &str, want: &str) -> bool {
    match (channel_path(current), channel_path(want)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}

/// The channel path out of a `mumble://` URL, decoded and trimmed of empty segments.
pub fn channel_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("mumble://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    // Everything after the host is the channel path; the host may carry credentials and a port.
    let (_, path) = rest.split_once('/')?;
    let decoded: Vec<String> =
        path.split('/').filter(|s| !s.is_empty()).map(percent_decode).collect();
    (!decoded.is_empty()).then(|| decoded.join("/"))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The channel path survives encoding, a port, a query string and a trailing slash.
    #[test]
    fn a_channel_path_comes_out_of_the_url() {
        assert_eq!(
            channel_path("mumble://mumble.example.com/Ops/Command%20Sector%20Alpha/Command%203?title=X"),
            Some("Ops/Command Sector Alpha/Command 3".to_owned())
        );
        assert_eq!(
            channel_path("mumble://user@host:64738/Ops/Op%20Channels/OP%201%20-%20Something/"),
            Some("Ops/Op Channels/OP 1 - Something".to_owned())
        );
        assert_eq!(channel_path("mumble://host/"), None);
        assert_eq!(channel_path("https://example.com/Ops/Thing"), None);
        assert_eq!(channel_path(""), None);
    }

    /// Two links to the same channel match however they are spelled; two different ones do not.
    #[test]
    fn the_same_channel_matches_across_spellings() {
        let want = "mumble://mumble.goonfleet.com/Ops/Command%20Sector%20Alpha/Command%203?title=Goonfleet&version=1.2.0";
        assert!(in_channel(
            "mumble://someone@voice.goonfleet.com:64738/Ops/Command+Sector+Alpha/Command+3",
            want
        ));
        assert!(!in_channel("mumble://host/Ops/Command%20Sector%20Bravo/Command%203", want));
        assert!(!in_channel("mumble://host/Root", want));
        // Nothing to compare against is not a match.
        assert!(!in_channel("", want));
        assert!(!in_channel(want, ""));
    }
}

/// Reads the running Mumble's state, changing nothing:
/// `cargo test --bin eve-spai mumble_status_live -- --ignored --nocapture`.
#[cfg(all(test, target_os = "linux"))]
#[test]
#[ignore = "needs a running Mumble"]
fn mumble_status_live() {
    println!("{:?} in {:?}", status(), current_url().and_then(|u| channel_path(&u)));
    assert!(status().is_some());
}
