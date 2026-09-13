//! The listener.
//!
//! `tiny_http` rather than a second HTTP stack: it is already a dependency, it already serves the
//! ESI login callback, it is synchronous, and the app has no shared async runtime to borrow. A bind
//! failure disables the feature and says so, the way `instance::start_control_listener` does; it
//! never takes the app down with it.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::routes::{self, Access, Route};
use super::sse::SharedHub;
use super::state::SharedWeb;

const WORKERS: usize = 4;

/// Failed pairing attempts allowed per address per window. The token is 256 bits, so this is log
/// hygiene rather than a real guessing defence, and it is deliberately generous enough that a phone
/// reloading a stale bookmark does not lock itself out.
const MAX_FAILS: u32 = 10;
const FAIL_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub allow_writeback: bool,
    pub bind_lan: bool,
    pub token: String,
    pub theme: crate::theme::Theme,
    /// Built once on the worker that first asks for it, then cached: the SDE does not change while
    /// the app is running, and walking 8000 systems per request would be silly.
    pub map: Option<Arc<super::map::Geometry>>,
}

pub struct Handle {
    server: Arc<tiny_http::Server>,
    running: Arc<AtomicBool>,
    hub: SharedHub,
    /// The address actually bound, for the pairing link the settings pane shows.
    #[allow(dead_code)]
    pub addr: String,
}

impl Handle {
    /// Devices holding a live stream. Surfaced in settings so a user can tell whether the phone in
    /// their hand is the thing that is connected.
    #[allow(dead_code)]
    pub fn clients(&self) -> usize {
        self.hub.client_count()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.server.unblock();
        // The listener stopping does not end the streams it already handed out, and a restart is
        // the normal way this drops. Tell them, so each page reconnects to the new listener instead
        // of holding a socket nothing will ever write to again.
        self.hub.close_all("restart");
    }
}

struct Ctx {
    cfg: Config,
    web: SharedWeb,
    /// What the dialogs read and where a write-back lands. Both are handles the app already owns.
    detail: super::Detail,
    inbox: super::Inbox,
    hub: SharedHub,
    map_json: Mutex<Option<Arc<str>>>,
    fails: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

/// `None` when the port could not be bound. The caller reports that and leaves the feature off.
pub fn start(
    cfg: Config,
    web: SharedWeb,
    detail: super::Detail,
    inbox: super::Inbox,
) -> Result<Handle, String> {
    let host = if cfg.bind_lan { "0.0.0.0" } else { "127.0.0.1" };
    let server = tiny_http::Server::http((host, cfg.port))
        .map_err(|e| format!("could not bind {host}:{} ({e})", cfg.port))?;
    let server = Arc::new(server);
    let running = Arc::new(AtomicBool::new(true));
    // Read back rather than echoing the setting, so port 0 resolves to what was actually bound.
    let addr = server.server_addr().to_string();

    let hub: SharedHub = Arc::new(super::sse::Hub::default());
    super::sse::spawn_broadcaster(hub.clone(), web.clone());
    let ctx = Arc::new(Ctx {
        cfg,
        web,
        detail,
        inbox,
        hub: hub.clone(),
        map_json: Mutex::new(None),
        fails: Mutex::new(HashMap::new()),
    });
    for _ in 0..WORKERS {
        let server = server.clone();
        let running = running.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            while running.load(Ordering::Acquire) {
                match server.recv_timeout(Duration::from_millis(500)) {
                    Ok(Some(req)) => handle(&ctx, req),
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
    }
    Ok(Handle { server, running, hub, addr })
}

/// This machine's address on the network it would route out of.
///
/// A UDP socket "connected" to a documentation address sends no packets; the kernel simply picks the
/// interface it would use and the local address falls out of that. Enumerating interfaces instead
/// means guessing which of several is the one the phone can reach.
pub fn lan_address() -> Option<std::net::IpAddr> {
    let sock = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    // TEST-NET-1: reserved for documentation and never routed anywhere.
    sock.connect(("192.0.2.1", 80)).ok()?;
    let ip = sock.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

fn header<'a>(req: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn handle(ctx: &Ctx, req: tiny_http::Request) {
    let (path, query) = routes::split_url(req.url());
    let (path, query) = (path.to_owned(), query.to_owned());
    let route = routes::classify(req.method().as_str(), &path);

    let host = header(&req, "Host").map(str::to_owned);
    let cookie_hdr = header(&req, "Cookie").map(str::to_owned);
    let cookie_token = cookie_hdr.as_deref().and_then(|c| routes::cookie(c, "spai"));
    let query_token = routes::query_param(&query, "t");
    let peer = req.remote_addr().map(|a| a.ip());

    let limited = peer.is_some_and(|ip| is_limited(ctx, ip));
    let access = routes::authorize(
        &route,
        query_token,
        cookie_token,
        host.as_deref(),
        &ctx.cfg.token,
        limited,
    );

    // A write has to be read before anything else touches the request, and it is the one place
    // `Origin` matters: a cross-site form post carries the browser's own cookie.
    if route == Route::Action {
        return match access {
            Access::Granted => {
                let origin = header(&req, "Origin").map(str::to_owned);
                act(ctx, req, origin.as_deref())
            }
            _ => respond(req, 403, "text/plain; charset=utf-8", b"not paired\n", &[]),
        };
    }

    match access {
        Access::Granted => serve(ctx, req, route, &path, &query),
        Access::Pair => {
            if let Some(ip) = peer {
                ctx.fails.lock().unwrap_or_else(|e| e.into_inner()).remove(&ip);
            }
            let cookie = pair_cookie(query_token.unwrap_or_default());
            // Served directly rather than through a 302.
            //
            // The redirect was the bug: a phone arriving from a QR scanner has no same-site
            // initiator, so the browser would not attach the just-set cookie to the redirect it was
            // told to follow, and the device landed on the "not paired" page having just paired.
            // The page strips the token from the address bar itself, which is what the redirect was
            // for and is one fewer thing to get wrong.
            let boot = ctx.web.lock().unwrap_or_else(|e| e.into_inner()).full_json();
            let page = super::assets::index_with_boot(&boot);
            respond(req, 200, "text/html; charset=utf-8", page.as_bytes(), &[
                ("Cache-Control", "no-store".to_owned()),
                ("Set-Cookie", cookie),
            ]);
        }
        Access::WrongToken => {
            if let Some(ip) = peer {
                note_failure(ctx, ip);
            }
            respond(req, 403, "text/html; charset=utf-8", STALE_PAGE.as_bytes(), &[]);
        }
        Access::Denied => {
            if let Some(ip) = peer {
                note_failure(ctx, ip);
            }
            respond(req, 403, "text/html; charset=utf-8", PAIR_PAGE.as_bytes(), &[]);
        }
        Access::RebindBlocked => {
            respond(req, 403, "text/plain; charset=utf-8", b"bad host\n", &[])
        }
        Access::RateLimited => respond(
            req,
            429,
            "text/plain; charset=utf-8",
            b"too many attempts\n",
            &[("Retry-After", "60".to_owned())],
        ),
    }
}

fn is_limited(ctx: &Ctx, ip: IpAddr) -> bool {
    let fails = ctx.fails.lock().unwrap_or_else(|e| e.into_inner());
    fails.get(&ip).is_some_and(|(at, n)| at.elapsed() < FAIL_WINDOW && *n >= MAX_FAILS)
}

fn note_failure(ctx: &Ctx, ip: IpAddr) {
    let mut fails = ctx.fails.lock().unwrap_or_else(|e| e.into_inner());
    fails.retain(|_, (at, _)| at.elapsed() < FAIL_WINDOW);
    let e = fails.entry(ip).or_insert((Instant::now(), 0));
    if e.0.elapsed() >= FAIL_WINDOW {
        *e = (Instant::now(), 0);
    }
    e.1 += 1;
}

fn serve(ctx: &Ctx, req: tiny_http::Request, route: Route, path: &str, query: &str) {
    let inm = header(&req, "If-None-Match").map(str::to_owned);
    match route {
        Route::Health => respond(req, 200, "text/plain; charset=utf-8", b"ok\n", &[]),
        Route::Index => {
            // Carries the live snapshot, so it is never cached.
            let boot = ctx.web.lock().unwrap_or_else(|e| e.into_inner()).full_json();
            let page = super::assets::index_with_boot(&boot);
            respond(req, 200, "text/html; charset=utf-8", page.as_bytes(), &[(
                "Cache-Control",
                "no-store".to_owned(),
            )])
        }
        Route::Asset => {
            match super::assets::find(path) {
                Some(a) => {
                    let (mime, body) = (a.mime, a.body.as_bytes());
                    cached(req, inm, mime, body)
                }
                None => respond(req, 404, "text/plain; charset=utf-8", b"not found\n", &[]),
            }
        }
        Route::Font => {
            // Version-stamped in its own URL, so it can be cached forever and still change on an
            // upgrade.
            respond(
                req,
                200,
                "font/ttf",
                super::assets::phosphor_ttf(),
                &[("Cache-Control", "public, max-age=31536000, immutable".to_owned())],
            )
        }
        Route::ThemeCss => {
            let theme = super::css::theme_from_query(query, &ctx.cfg.theme);
            respond(req, 200, "text/css; charset=utf-8", super::css::theme_css(&theme).as_bytes(), &[
                ("Cache-Control", "no-store".to_owned()),
            ])
        }
        Route::Icons => cached(req, inm, "application/json; charset=utf-8", super::icons::json().as_bytes()),
        Route::Events => {
            let since = header(&req, "Last-Event-ID").and_then(|v| v.trim().parse::<u64>().ok());
            super::sse::serve(req, ctx.hub.clone(), ctx.web.clone(), since)
        }
        Route::MapGeometry => {
            let json = {
                let mut slot = ctx.map_json.lock().unwrap_or_else(|e| e.into_inner());
                slot.get_or_insert_with(|| match &ctx.cfg.map {
                    Some(g) => serde_json::to_string(g.as_ref()).unwrap_or_default().into(),
                    None => Arc::from("{\"extent\":4096,\"nodes\":[],\"edges\":[]}"),
                })
                .clone()
            };
            // Keyed on the content, so a rebuilt SDE serves a new tag and an unchanged one does not.
            cached(req, inm, "application/json; charset=utf-8", json.as_bytes())
        }
        Route::State => {
            // The fallback for anything that mangles an event stream: the same serializer, asked
            // for rather than pushed. It answers immediately with whatever changed since `since`,
            // and does NOT hold the connection open. Long-polling was the original plan and is the
            // wrong shape here: there are four workers, so a handful of parked phones would starve
            // every other request on the server.
            let since = routes::query_param(query, "since")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let snap = ctx.web.lock().unwrap_or_else(|e| e.into_inner()).snapshot_since(since);
            let json = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".to_owned());
            respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[(
                "Cache-Control",
                "no-store".to_owned(),
            )])
        }
        Route::JabberChat => {
            // Percent-decoded, because a JID's `@` is encoded by the page and a raw one would look
            // up a conversation that does not exist.
            let jid = routes::query_param(query, "jid").map(routes::percent_decode).unwrap_or_default();
            let state = ctx.detail.lock().unwrap_or_else(|e| e.into_inner()).jabber.clone();
            let out = match state {
                Some(j) => {
                    let st = j.lock().unwrap_or_else(|e| e.into_inner());
                    super::jabber::chat(&st, &jid, 200)
                }
                None => super::jabber::chat(&crate::jabber::JabberState::default(), &jid, 0),
            };
            let json = serde_json::to_string(&out).unwrap_or_else(|_| "{}".to_owned());
            respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[(
                "Cache-Control",
                "no-store".to_owned(),
            )])
        }
        Route::Snapshot => {
            let json = ctx.web.lock().unwrap_or_else(|e| e.into_inner()).full_json();
            respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[
                ("Cache-Control", "no-store".to_owned()),
            ])
        }
        Route::NotAllowed => respond(req, 405, "text/plain; charset=utf-8", b"method not allowed\n", &[]),
        Route::Sound(name) => match crate::sound::preset_wav(&name) {
            Some(bytes) => respond(req, 200, "audio/wav", &bytes, &[(
                "Cache-Control",
                "public, max-age=31536000, immutable".to_owned(),
            )]),
            None => respond(req, 404, "text/plain; charset=utf-8", b"no such sound\n", &[]),
        },
        Route::SystemInfo(id) => {
            let d = ctx.detail.lock().unwrap_or_else(|e| e.into_inner());
            let body = d.graph.as_ref().and_then(|g| {
                super::detail::system(
                    id,
                    g,
                    d.status.get(&id),
                    d.player_sys,
                    d.count_bridges,
                )
            });
            json_or_404(req, body)
        }
        Route::ShipInfo(id) => {
            let d = ctx.detail.lock().unwrap_or_else(|e| e.into_inner());
            let body = d.store.as_ref().and_then(|s| super::detail::ship(id, s));
            json_or_404(req, body)
        }
        Route::Action => unreachable!("answered before `serve`, which cannot read a body"),
        Route::NotFound => respond(req, 404, "text/plain; charset=utf-8", b"not found\n", &[]),
    }
}

/// Queue one action from the page.
fn act(ctx: &Ctx, mut req: tiny_http::Request, origin: Option<&str>) {
    if !ctx.cfg.allow_writeback {
        return respond(req, 403, "text/plain; charset=utf-8", b"read only\n", &[]);
    }
    if !routes::origin_allowed(origin) {
        return respond(req, 403, "text/plain; charset=utf-8", b"bad origin\n", &[]);
    }
    let mut body = String::new();
    if std::io::Read::read_to_string(&mut req.as_reader(), &mut body).is_err() {
        return respond(req, 400, "text/plain; charset=utf-8", b"unreadable\n", &[]);
    }
    match serde_json::from_str::<crate::ipc::OverlayToMain>(&body) {
        Ok(msg) => {
            ctx.inbox.lock().unwrap_or_else(|e| e.into_inner()).push(msg);
            respond(req, 204, "text/plain; charset=utf-8", b"", &[])
        }
        Err(_) => respond(req, 400, "text/plain; charset=utf-8", b"bad action\n", &[]),
    }
}

fn json_or_404<T: serde::Serialize>(req: tiny_http::Request, body: Option<T>) {
    match body.and_then(|b| serde_json::to_string(&b).ok()) {
        Some(json) => respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[(
            "Cache-Control",
            "no-store".to_owned(),
        )]),
        None => respond(req, 404, "application/json; charset=utf-8", b"{}", &[]),
    }
}

fn cached(req: tiny_http::Request, if_none_match: Option<String>, mime: &str, body: &[u8]) {
    let tag = super::assets::etag(body);
    // `no-cache` means "ask me first", not "do not store": the browser keeps the copy and
    // revalidates, so an unchanged asset still costs one 304 and nothing more.
    //
    // Without it there is no freshness information at all, and a browser is free to apply its own
    // heuristic and serve a stale copy without asking. That is exactly what happened: a fixed page
    // kept rendering the old behaviour on the one machine that had loaded it before.
    let head = [("Cache-Control", "no-cache".to_owned()), ("ETag", tag.clone())];
    if if_none_match.as_deref() == Some(tag.as_str()) {
        return respond(req, 304, mime, b"", &head);
    }
    respond(req, 200, mime, body, &head)
}

fn respond(req: tiny_http::Request, status: u16, mime: &str, body: &[u8], extra: &[(&str, String)]) {
    let mut headers = vec![hdr("Content-Type", mime)];
    for (k, v) in extra {
        headers.push(hdr(k, v));
    }
    let resp = tiny_http::Response::new(
        tiny_http::StatusCode(status),
        headers,
        std::io::Cursor::new(body.to_vec()),
        Some(body.len()),
        None,
    );
    let _ = req.respond(resp);
}

fn hdr(k: &str, v: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes())
        .unwrap_or_else(|()| tiny_http::Header::from_bytes(&b"X-Bad"[..], &b"1"[..]).expect("static"))
}

/// `Lax`, not `Strict`.
///
/// `Strict` withholds the cookie on any navigation that did not start on this site, and a QR scan
/// starts outside the browser entirely, so the very first page load after pairing arrived without
/// it. `Lax` attaches it to top-level GET navigations, which is exactly this case, and still
/// withholds it from cross-site POSTs. Writes do not lean on that anyway: `/api/action` checks
/// `Origin`, which is the defence that actually holds.
fn pair_cookie(token: &str) -> String {
    format!("spai={token}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax")
}

const STALE_PAGE: &str = r#"<!doctype html><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>EVE Spai</title>
<style>body{font-family:system-ui;background:#0b0f12;color:#c8d2d8;display:flex;height:100vh;
margin:0;align-items:center;justify-content:center;text-align:center}p{color:#7a848b;max-width:28rem}</style>
<div><h2>EVE Spai</h2><p>That pairing link is not valid any more. It was probably regenerated.
Open Settings on the desktop, reveal the pairing link, and scan the new code.</p></div>
"#;

const PAIR_PAGE: &str = r#"<!doctype html><meta charset=utf-8>
<meta name=viewport content="width=device-width,initial-scale=1">
<title>EVE Spai</title>
<style>body{font-family:system-ui;background:#0b0f12;color:#c8d2d8;display:flex;height:100vh;
margin:0;align-items:center;justify-content:center;text-align:center}p{color:#7a848b;max-width:28rem}</style>
<div><h2>EVE Spai</h2><p>This device is not paired. Open the link from the app's settings, which
carries the pairing token.</p></div>
"#;

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const TOKEN: &str = "test-token-aaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Running {
        _handle: Handle,
        base: String,
        web: crate::web::state::SharedWeb,
        inbox: crate::web::Inbox,
    }

    fn serve_test() -> Running {
        let web = crate::web::state::shared();
        let inbox = crate::web::inbox();
        let handle = start(
            Config {
                port: 0,
                allow_writeback: true,
                bind_lan: false,
                token: TOKEN.to_owned(),
                theme: crate::theme::Theme::caldari(),
                map: None,
            },
            web.clone(),
            crate::web::detail(),
            inbox.clone(),
        )
        .expect("bind an ephemeral loopback port");
        let base = format!("http://{}", handle.addr);
        Running { _handle: handle, base, web, inbox }
    }

    fn client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client")
    }

    fn get(c: &reqwest::blocking::Client, url: &str) -> reqwest::blocking::Response {
        c.get(url).send().expect("request")
    }

    /// Either an address a phone could use, or nothing. Never loopback or 0.0.0.0, which would be a
    /// link that silently cannot work from another device.
    #[test]
    fn the_lan_address_is_usable_or_absent() {
        match lan_address() {
            Some(ip) => {
                assert!(!ip.is_loopback(), "{ip} is this machine only");
                assert!(!ip.is_unspecified(), "{ip} is not an address");
            }
            // A machine with no route out is a legitimate answer, and the caller says so.
            None => {}
        }
    }

    #[test]
    fn the_pairing_round_trip() {
        let s = serve_test();
        let c = client();

        assert_eq!(get(&c, &s.base).status(), 403, "an unpaired device sees nothing");
        assert_eq!(
            get(&c, &format!("{}/?t=wrong", s.base)).status(),
            403,
            "a wrong token is no better than none"
        );

        let paired = get(&c, &format!("{}/?t={TOKEN}", s.base));
        assert_eq!(paired.status(), 200, "a good token serves the page and sets the cookie");
        let cookie = paired.headers().get("set-cookie").unwrap().to_str().unwrap().to_owned();
        assert!(cookie.starts_with(&format!("spai={TOKEN}")));
        assert!(cookie.contains("HttpOnly"), "script must not be able to read the token");

        let page = c.get(&s.base).header("Cookie", &cookie).send().expect("request");
        assert_eq!(page.status(), 200);
        assert!(page.text().unwrap().contains("EVE Spai"));
    }

    /// The reported bug: a phone scanned the QR, reached the server, and was told it was not paired.
    ///
    /// Pairing used to answer 302 with a `SameSite=Strict` cookie. A QR scan has no same-site
    /// initiator, so the browser withheld the cookie from the redirect it had just been told to
    /// follow, and the device landed on the pair page having just paired.
    #[test]
    fn pairing_serves_the_page_itself_rather_than_a_redirect() {
        let s = serve_test();
        let c = client();
        let r = get(&c, &format!("{}/?t={TOKEN}", s.base));

        assert_eq!(r.status(), 200, "pairing must not depend on a redirect being followed");
        let cookie = r.headers().get("set-cookie").unwrap().to_str().unwrap().to_owned();
        assert!(cookie.contains("SameSite=Lax"), "Strict is withheld on a scan: {cookie}");
        assert!(cookie.contains("HttpOnly"), "script must not be able to read the token");
        assert!(!cookie.contains("SameSite=Strict"));
        assert!(r.text().unwrap().contains("EVE Spai"), "the page itself has to come back");
    }

    /// A regenerated link and a device that never paired are different problems, and the page says
    /// which.
    #[test]
    fn a_stale_token_says_so() {
        let s = serve_test();
        let c = client();
        let stale = get(&c, &format!("{}/?t=an-old-token", s.base));
        assert_eq!(stale.status(), 403);
        assert!(stale.text().unwrap().contains("not valid any more"));

        let never = get(&c, &s.base);
        assert_eq!(never.status(), 403);
        assert!(never.text().unwrap().contains("not paired"));
    }

    #[test]
    fn health_answers_without_pairing_and_everything_else_does_not() {
        let s = serve_test();
        let c = client();
        assert_eq!(get(&c, &format!("{}/healthz", s.base)).status(), 200);
        for path in ["/", "/assets/app.js", "/api/snapshot", "/api/theme.css"] {
            assert_eq!(get(&c, &format!("{}{path}", s.base)).status(), 403, "{path}");
        }
    }

    /// The DNS-rebinding case. The attacker's page resolves a name they control to this machine, so
    /// the request arrives with their `Host`. It must fail even carrying a valid token, because by
    /// the time this matters the token is what they are trying to use.
    #[test]
    fn a_request_for_someone_elses_hostname_is_refused() {
        let s = serve_test();
        let c = client();
        for host in ["evil.com", "127.0.0.1.evil.com", "spai.attacker.test"] {
            let r = c
                .get(format!("{}/?t={TOKEN}", s.base))
                .header("Host", host)
                .send()
                .expect("request");
            assert_eq!(r.status(), 403, "{host} was allowed through");
            assert!(r.headers().get("set-cookie").is_none(), "{host} was paired");
        }
    }

    /// An asset with an `ETag` and no freshness information lets the browser decide on its own how
    /// long to keep it, which it does, and a fixed page goes on rendering the old bug.
    #[test]
    fn assets_are_revalidated_rather_than_heuristically_cached() {
        let s = serve_test();
        let c = client();
        let r = c
            .get(format!("{}/assets/app.js", s.base))
            .header("Cookie", format!("spai={TOKEN}"))
            .send()
            .unwrap();
        let cc = r.headers().get("cache-control").expect("no Cache-Control at all");
        assert_eq!(cc, "no-cache");
    }

    #[test]
    fn assets_revalidate_by_etag() {
        let s = serve_test();
        let c = client();
        let cookie = format!("spai={TOKEN}");

        let first =
            c.get(format!("{}/assets/app.js", s.base)).header("Cookie", &cookie).send().unwrap();
        assert_eq!(first.status(), 200);
        let tag = first.headers().get("etag").unwrap().to_str().unwrap().to_owned();

        let again = c
            .get(format!("{}/assets/app.js", s.base))
            .header("Cookie", &cookie)
            .header("If-None-Match", &tag)
            .send()
            .unwrap();
        assert_eq!(again.status(), 304, "an unchanged asset is not sent twice");
    }

    #[test]
    fn the_theme_sheet_follows_the_query() {
        let s = serve_test();
        let c = client();
        let cookie = format!("spai={TOKEN}");

        let dflt = c
            .get(format!("{}/api/theme.css", s.base))
            .header("Cookie", &cookie)
            .send()
            .unwrap()
            .text()
            .unwrap();
        assert!(dflt.contains("--accent: #3fa9c9"), "the app's own accent");

        let custom = c
            .get(format!("{}/api/theme.css?accent=ff0000", s.base))
            .header("Cookie", &cookie)
            .send()
            .unwrap()
            .text()
            .unwrap();
        assert!(custom.contains("--accent: #ff0000"));
        assert!(custom.contains("--fg: #c8d2d8"), "an override replaces only what it supplies");
    }

    #[test]
    fn repeated_bad_tokens_are_rate_limited() {
        let s = serve_test();
        let c = client();
        for _ in 0..MAX_FAILS {
            assert_eq!(get(&c, &format!("{}/?t=wrong", s.base)).status(), 403);
        }
        let blocked = get(&c, &format!("{}/?t={TOKEN}", s.base));
        assert_eq!(blocked.status(), 429, "a good token cannot walk past the limiter");
        assert_eq!(blocked.headers().get("retry-after").unwrap(), "60");
    }

    #[test]
    fn unknown_paths_and_wrong_methods_are_answered_not_hung() {
        let s = serve_test();
        let c = client();
        let cookie = format!("spai={TOKEN}");
        let r = c.get(format!("{}/nope", s.base)).header("Cookie", &cookie).send().unwrap();
        assert_eq!(r.status(), 404);
        let r = c.post(format!("{}/", s.base)).header("Cookie", &cookie).send().unwrap();
        assert_eq!(r.status(), 405);
    }

    fn open_stream(base: &str) -> std::net::TcpStream {
        let addr = base.trim_start_matches("http://").to_owned();
        let mut sock = std::net::TcpStream::connect(&addr).expect("connect");
        let host = addr.clone();
        write!(
            sock,
            "GET /api/events HTTP/1.1\r\nHost: {host}\r\nCookie: spai={TOKEN}\r\n\r\n"
        )
        .expect("request");
        sock.set_read_timeout(Some(Duration::from_millis(2500))).expect("timeout");
        sock
    }

    /// Read until `pat` shows up, or give up. Returns how long it took.
    fn wait_for(sock: &mut std::net::TcpStream, pat: &str) -> Option<Duration> {
        use std::io::Read;
        let start = Instant::now();
        let mut seen = String::new();
        let mut buf = [0u8; 2048];
        while start.elapsed() < Duration::from_millis(2500) {
            match sock.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    seen.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if seen.contains(pat) {
                        return Some(start.elapsed());
                    }
                }
                Err(_) => break,
            }
        }
        None
    }

    /// The regression test for the whole reason `sse.rs` writes its own chunks.
    ///
    /// `tiny_http`'s unknown-length response buffers 8 KB and never flushes per write, so the
    /// obvious implementation delivers nothing until roughly forty events have piled up. Against
    /// that version this test times out; the framing in `sse::serve` is what makes it pass.
    #[test]
    fn first_event_arrives_promptly() {
        let s = serve_test();
        let mut sock = open_stream(&s.base);
        let took = wait_for(&mut sock, "data: ").expect("no event reached the socket");
        assert!(took < Duration::from_millis(500), "first event took {took:?}");
    }

    #[test]
    fn a_later_publish_reaches_an_open_stream() {
        let s = serve_test();
        let mut sock = open_stream(&s.base);
        wait_for(&mut sock, "data: ").expect("opening snapshot");

        {
            let mut st = s.web.lock().unwrap();
            let rev = st.changed(crate::web::state::Pane::Map, 12345).expect("a fresh hash");
            st.put_map(crate::web::snapshot::MapLive { rev, you: Some(30_004_759), ..Default::default() });
        }
        let took = wait_for(&mut sock, "30004759").expect("the change never arrived");
        assert!(took < Duration::from_millis(900), "a publish took {took:?} to reach the stream");
    }

    /// WEB-004's fallback for a network that mangles event streams.
    #[test]
    fn state_answers_a_delta_and_does_not_hold_the_connection() {
        let s = serve_test();
        let c = client();
        let cookie = format!("spai={TOKEN}");
        let get_state = |since: u64| {
            let started = Instant::now();
            let body = c
                .get(format!("{}/api/state?since={since}", s.base))
                .header("Cookie", &cookie)
                .send()
                .unwrap()
                .text()
                .unwrap();
            (body, started.elapsed())
        };

        {
            let mut st = s.web.lock().unwrap();
            let rev = st.changed(crate::web::state::Pane::Map, 777).expect("fresh");
            st.put_map(crate::web::snapshot::MapLive {
                rev,
                you: Some(30_004_759),
                ..Default::default()
            });
        }
        let seq = s.web.lock().unwrap().seq;

        let (fresh, took) = get_state(0);
        assert!(fresh.contains("30004759"), "a client with nothing gets everything");
        assert!(took < Duration::from_secs(1), "it must not park a worker: took {took:?}");

        let (caught_up, took) = get_state(seq);
        assert!(!caught_up.contains("30004759"), "a caught-up client is sent no pane");
        assert!(caught_up.contains(&format!("\"seq\":{seq}")), "but is still told where it is");
        assert!(took < Duration::from_secs(1), "took {took:?}");
    }

    fn post(c: &reqwest::blocking::Client, base: &str, origin: Option<&str>, body: &str)
        -> reqwest::blocking::Response {
        let mut r = c
            .post(format!("{base}/api/action"))
            .header("Cookie", format!("spai={TOKEN}"))
            .body(body.to_owned());
        if let Some(o) = origin {
            r = r.header("Origin", o);
        }
        r.send().expect("request")
    }

    #[test]
    fn an_action_from_the_page_lands_in_the_inbox() {
        let s = serve_test();
        let c = client();
        let origin = Some(s.base.as_str());

        let ok = post(&c, &s.base, origin, r#"{"Verdict":{"name":"Hostile Pilot","hidden":true}}"#);
        assert_eq!(ok.status(), 204);
        let queued = s.inbox.lock().unwrap();
        assert_eq!(queued.len(), 1);
        assert!(matches!(
            &queued[0],
            crate::ipc::OverlayToMain::Verdict { name, hidden: true } if name == "Hostile Pilot"
        ));
    }

    /// The cookie is `SameSite=Strict`, so a cross-site post should not carry it at all. The origin
    /// check is the second lock on the same door, and it is cheap.
    /// The page asks the desktop to join comms; it never sends a URL. A message carrying one would
    /// be a way to make this machine open anything, from the network.
    #[test]
    fn join_comms_names_a_ping_and_never_a_url() {
        let s = serve_test();
        let c = client();
        let origin = Some(s.base.as_str());

        assert_eq!(post(&c, &s.base, origin, r#"{"JoinComms":{"ts":1789173781}}"#).status(), 204);
        assert!(matches!(
            &s.inbox.lock().unwrap()[0],
            crate::ipc::OverlayToMain::JoinComms { ts: 1789173781 }
        ));

        // Nothing in the protocol accepts a link.
        for body in [
            r#"{"JoinComms":{"url":"mumble://evil"}}"#,
            r#"{"JoinComms":{"ts":"file:///etc/passwd"}}"#,
            r#"{"OpenUrl":{"url":"http://evil"}}"#,
        ] {
            assert_eq!(post(&c, &s.base, origin, body).status(), 400, "{body}");
        }
        assert_eq!(s.inbox.lock().unwrap().len(), 1);
    }

    /// Selecting a system on the phone moves the desktop map. Like `JoinComms`, it names a thing the
    /// app already has rather than carrying anything the app then acts on blindly.
    #[test]
    fn select_system_names_an_id() {
        let s = serve_test();
        let c = client();
        let origin = Some(s.base.as_str());
        assert_eq!(post(&c, &s.base, origin, r#"{"SelectSystem":{"id":30004759}}"#).status(), 204);
        assert!(matches!(
            &s.inbox.lock().unwrap()[0],
            crate::ipc::OverlayToMain::SelectSystem { id: 30_004_759 }
        ));
    }

    #[test]
    fn a_post_from_somewhere_else_is_refused() {
        let s = serve_test();
        let c = client();
        for origin in [Some("http://evil.com"), Some("http://127.0.0.1.evil.com"), None] {
            let r = post(&c, &s.base, origin, r#"{"AlertAck":{"id":1}}"#);
            assert_eq!(r.status(), 403, "{origin:?} was accepted");
        }
        assert!(s.inbox.lock().unwrap().is_empty());
    }

    #[test]
    fn a_malformed_action_is_rejected_rather_than_queued() {
        let s = serve_test();
        let c = client();
        let origin = Some(s.base.as_str());
        assert_eq!(post(&c, &s.base, origin, "not json").status(), 400);
        assert_eq!(post(&c, &s.base, origin, r#"{"Nonsense":1}"#).status(), 400);
        assert!(s.inbox.lock().unwrap().is_empty());
    }

    #[test]
    fn the_dialog_endpoints_answer_or_404() {
        let s = serve_test();
        let c = client();
        let cookie = format!("spai={TOKEN}");
        // The test server carries no graph and no store, so these are the not-found paths.
        for path in ["/api/system/30004759", "/api/ship/587"] {
            let r = c.get(format!("{}{path}", s.base)).header("Cookie", &cookie).send().unwrap();
            assert_eq!(r.status(), 404, "{path}");
        }
        let r = c
            .get(format!("{}/api/system/not-a-number", s.base))
            .header("Cookie", &cookie)
            .send()
            .unwrap();
        assert_eq!(r.status(), 404);
    }

    #[test]
    fn the_ninth_stream_is_turned_away() {
        let s = serve_test();
        let held: Vec<_> = (0..crate::web::sse::MAX_CLIENTS).map(|_| {
            let mut sock = open_stream(&s.base);
            wait_for(&mut sock, "data: ").expect("opening snapshot");
            sock
        }).collect();

        let c = client();
        let over = c
            .get(format!("{}/api/events", s.base))
            .header("Cookie", format!("spai={TOKEN}"))
            .send()
            .expect("request");
        assert_eq!(over.status(), 503);
        assert_eq!(over.headers().get("retry-after").unwrap(), "5");
        drop(held);
    }

    #[test]
    fn dropping_the_handle_frees_the_port() {
        let addr = {
            let s = serve_test();
            assert_eq!(get(&client(), &format!("{}/healthz", s.base)).status(), 200);
            s.base
        };
        // The workers unblock on their own timer, so give them a moment before expecting silence.
        std::thread::sleep(Duration::from_millis(800));
        assert!(
            client().get(format!("{addr}/healthz")).send().is_err(),
            "the listener outlived its handle"
        );
    }
}
