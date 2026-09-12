//! Routing and access control, as pure functions.
//!
//! Everything here answers from its arguments alone, so the table of "which request gets which
//! answer" is testable without a socket. The socket side lives in [`super::server`].

use std::net::IpAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Index,
    /// A file from the static table, by path.
    Asset,
    ThemeCss,
    Icons,
    Font,
    Snapshot,
    State,
    MapGeometry,
    Events,
    Health,
    NotFound,
    NotAllowed,
}

/// Whether a route is reachable without pairing. Only the health check is: everything else is
/// either the intel itself or a page that immediately asks for it.
pub fn is_public(r: Route) -> bool {
    matches!(r, Route::Health)
}

pub fn classify(method: &str, path: &str) -> Route {
    if !matches!(method, "GET" | "HEAD") {
        return Route::NotAllowed;
    }
    match path {
        "/" | "/index.html" => Route::Index,
        "/healthz" => Route::Health,
        "/api/theme.css" => Route::ThemeCss,
        "/api/icons.json" => Route::Icons,
        "/api/snapshot" => Route::Snapshot,
        "/api/state" => Route::State,
        "/api/map/geometry" => Route::MapGeometry,
        "/api/events" => Route::Events,
        p if p.starts_with("/assets/phosphor-") && p.ends_with(".ttf") => Route::Font,
        p if super::assets::find(p).is_some() => Route::Asset,
        _ => Route::NotFound,
    }
}

/// The path without its query string, and the query as given.
pub fn split_url(url: &str) -> (&str, &str) {
    match url.split_once('?') {
        Some((p, q)) => (p, q),
        None => (url, ""),
    }
}

pub fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then_some(v)
    })
}

pub fn cookie<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    header.split(';').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k.trim() == key).then(|| v.trim())
    })
}

/// The `Host` a request claims to have been sent to.
///
/// This is the whole DNS-rebinding defence, and it is why the check parses rather than matches: an
/// attacker's page reaches a LAN socket by resolving a name they control to this machine's address,
/// and that request carries their hostname. `127.0.0.1.evil.com` is a real name that a `starts_with`
/// or a `contains` would wave straight through.
pub fn host_allowed(host: Option<&str>) -> bool {
    let Some(h) = host.map(str::trim).filter(|h| !h.is_empty()) else {
        return false;
    };
    let name = if let Some(rest) = h.strip_prefix('[') {
        match rest.split_once(']') {
            Some((inner, _)) => inner,
            None => return false,
        }
    } else {
        match h.rsplit_once(':') {
            Some((a, port)) if !port.is_empty() && port.bytes().all(|c| c.is_ascii_digit()) => a,
            _ => h,
        }
    };
    name.eq_ignore_ascii_case("localhost") || name.parse::<IpAddr>().is_ok()
}

/// The same test, applied to an `Origin`. Required on writes, where a cross-site form post is the
/// thing being stopped. Written and tested here with the rest of the access rules; WEB-008 is what
/// brings the first write for it to guard.
#[allow(dead_code)]
pub fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(o) = origin.map(str::trim) else { return false };
    let rest = o.strip_prefix("http://").or_else(|| o.strip_prefix("https://"));
    match rest {
        Some(r) => host_allowed(Some(r.split('/').next().unwrap_or(r))),
        None => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Already paired: serve it.
    Granted,
    /// A valid token arrived in the query. Set the cookie and redirect, so the token stops being in
    /// the address bar, in history and in any screenshot of the page.
    Pair,
    Denied,
    RebindBlocked,
    RateLimited,
}

pub fn authorize(
    route: Route,
    query_token: Option<&str>,
    cookie_token: Option<&str>,
    host: Option<&str>,
    expected: &str,
    rate_limited: bool,
) -> Access {
    if is_public(route) {
        return Access::Granted;
    }
    if !host_allowed(host) {
        return Access::RebindBlocked;
    }
    if expected.is_empty() {
        return Access::Denied;
    }
    if cookie_token.is_some_and(|t| super::auth::ct_eq(t, expected)) {
        return Access::Granted;
    }
    if rate_limited {
        return Access::RateLimited;
    }
    if query_token.is_some_and(|t| super::auth::ct_eq(t, expected)) {
        return Access::Pair;
    }
    Access::Denied
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_covers_the_table() {
        assert_eq!(classify("GET", "/"), Route::Index);
        assert_eq!(classify("GET", "/index.html"), Route::Index);
        assert_eq!(classify("GET", "/healthz"), Route::Health);
        assert_eq!(classify("GET", "/api/theme.css"), Route::ThemeCss);
        assert_eq!(classify("GET", "/api/icons.json"), Route::Icons);
        assert_eq!(classify("GET", "/api/snapshot"), Route::Snapshot);
        assert_eq!(classify("GET", "/api/events"), Route::Events);
        assert_eq!(classify("GET", "/api/state"), Route::State);
        assert_eq!(classify("GET", "/api/map/geometry"), Route::MapGeometry);
        assert_eq!(classify("GET", "/assets/app.js"), Route::Asset);
        assert_eq!(classify("GET", "/assets/phosphor-9.9.9.ttf"), Route::Font);
        assert_eq!(classify("GET", "/nope"), Route::NotFound);
        assert_eq!(classify("POST", "/"), Route::NotAllowed);
        assert_eq!(classify("DELETE", "/api/snapshot"), Route::NotAllowed);
    }

    #[test]
    fn only_the_health_check_is_public() {
        assert!(is_public(Route::Health));
        for r in [
            Route::Index,
            Route::Asset,
            Route::ThemeCss,
            Route::Snapshot,
            Route::State,
            Route::MapGeometry,
            Route::Events,
            Route::Font,
        ] {
            assert!(!is_public(r), "{r:?} must require pairing");
        }
    }

    #[test]
    fn host_accepts_loopback_and_literals_only() {
        for h in [
            "localhost",
            "LOCALHOST:6767",
            "127.0.0.1",
            "127.0.0.1:6767",
            "192.168.1.5:6767",
            "10.0.0.2",
            "[::1]:6767",
            "[fe80::1]",
        ] {
            assert!(host_allowed(Some(h)), "{h} should be allowed");
        }
        for h in [
            "evil.com",
            "evil.com:6767",
            "spai.local",
            // The classic rebinding bypass: a real hostname that merely starts with an address.
            "127.0.0.1.evil.com",
            "127.0.0.1.evil.com:6767",
            "192.168.1.5.attacker.test",
            "",
            "   ",
        ] {
            assert!(!host_allowed(Some(h)), "{h} should be refused");
        }
        assert!(!host_allowed(None), "a request with no Host is not a request we answer");
    }

    #[test]
    fn origin_is_tested_the_same_way() {
        assert!(origin_allowed(Some("http://192.168.1.5:6767")));
        assert!(origin_allowed(Some("http://localhost:6767")));
        assert!(!origin_allowed(Some("http://127.0.0.1.evil.com:6767")));
        assert!(!origin_allowed(Some("https://evil.com")));
        assert!(!origin_allowed(Some("null")), "an opaque origin is not this machine");
        assert!(!origin_allowed(None));
    }

    #[test]
    fn parses_query_and_cookie() {
        let (p, q) = split_url("/api/snapshot?since=4&t=abc");
        assert_eq!(p, "/api/snapshot");
        assert_eq!(query_param(q, "since"), Some("4"));
        assert_eq!(query_param(q, "t"), Some("abc"));
        assert_eq!(query_param(q, "nope"), None);
        assert_eq!(split_url("/"), ("/", ""));

        assert_eq!(cookie("spai=abc", "spai"), Some("abc"));
        assert_eq!(cookie("other=1; spai=abc; more=2", "spai"), Some("abc"));
        assert_eq!(cookie("other=1", "spai"), None);
    }

    #[test]
    fn authorize_walks_the_whole_table() {
        let tok = "secret-token";
        let host = Some("192.168.1.5:6767");

        assert_eq!(
            authorize(Route::Health, None, None, Some("evil.com"), tok, false),
            Access::Granted,
            "the health check answers before any of the checks"
        );
        assert_eq!(authorize(Route::Index, None, None, host, tok, false), Access::Denied);
        assert_eq!(
            authorize(Route::Index, Some("wrong"), None, host, tok, false),
            Access::Denied
        );
        assert_eq!(authorize(Route::Index, Some(tok), None, host, tok, false), Access::Pair);
        assert_eq!(authorize(Route::Index, None, Some(tok), host, tok, false), Access::Granted);
        assert_eq!(
            authorize(Route::Index, Some(tok), None, Some("evil.com"), tok, false),
            Access::RebindBlocked,
            "a rebound request must not pair, however good its token"
        );
        assert_eq!(
            authorize(Route::Index, Some(tok), None, host, "", false),
            Access::Denied,
            "no token configured means nothing is reachable"
        );
        assert_eq!(
            authorize(Route::Index, Some(tok), None, host, tok, true),
            Access::RateLimited
        );
        assert_eq!(
            authorize(Route::Index, None, Some(tok), host, tok, true),
            Access::Granted,
            "an already paired device is not rate limited"
        );
    }
}
