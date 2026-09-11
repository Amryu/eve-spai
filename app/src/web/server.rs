//! The listener.
//!
//! `tiny_http` rather than a second HTTP stack: it is already a dependency, it already serves the
//! ESI login callback, it is synchronous, and the app has no shared async runtime to borrow. A bind
//! failure disables the feature and says so, the way `instance::start_control_listener` does; it
//! never takes the app down with it.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::routes::{self, Access, Route};
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
    pub bind_lan: bool,
    pub token: String,
    pub theme: crate::theme::Theme,
}

pub struct Handle {
    server: Arc<tiny_http::Server>,
    running: Arc<AtomicBool>,
    clients: Arc<AtomicU32>,
    pub addr: String,
}

impl Handle {
    /// Devices that have completed a request recently. Surfaced in settings so a user can tell
    /// whether the phone in their hand is the thing that is connected.
    pub fn clients(&self) -> u32 {
        self.clients.load(Ordering::Relaxed)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.server.unblock();
    }
}

struct Ctx {
    cfg: Config,
    web: SharedWeb,
    fails: Mutex<HashMap<IpAddr, (Instant, u32)>>,
    clients: Arc<AtomicU32>,
}

/// `None` when the port could not be bound. The caller reports that and leaves the feature off.
pub fn start(cfg: Config, web: SharedWeb) -> Result<Handle, String> {
    let host = if cfg.bind_lan { "0.0.0.0" } else { "127.0.0.1" };
    let server = tiny_http::Server::http((host, cfg.port))
        .map_err(|e| format!("could not bind {host}:{} ({e})", cfg.port))?;
    let server = Arc::new(server);
    let running = Arc::new(AtomicBool::new(true));
    let clients = Arc::new(AtomicU32::new(0));
    // Read back rather than echoing the setting, so port 0 resolves to what was actually bound.
    let addr = server.server_addr().to_string();

    let ctx = Arc::new(Ctx {
        cfg,
        web,
        fails: Mutex::new(HashMap::new()),
        clients: clients.clone(),
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
    Ok(Handle { server, running, clients, addr })
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
        route,
        query_token,
        cookie_token,
        host.as_deref(),
        &ctx.cfg.token,
        limited,
    );

    match access {
        Access::Granted => {
            ctx.clients.store(1, Ordering::Relaxed);
            serve(ctx, req, route, &query)
        }
        Access::Pair => {
            if let Some(ip) = peer {
                ctx.fails.lock().unwrap_or_else(|e| e.into_inner()).remove(&ip);
            }
            let token = query_token.unwrap_or_default();
            // The token leaves the address bar as soon as it has been exchanged for a cookie, so it
            // stops living in history, in a shared screenshot and in any referrer.
            respond(
                req,
                302,
                "text/plain; charset=utf-8",
                b"",
                &[
                    ("Location", "/".to_owned()),
                    (
                        "Set-Cookie",
                        format!(
                            "spai={token}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Strict"
                        ),
                    ),
                ],
            );
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

fn serve(ctx: &Ctx, req: tiny_http::Request, route: Route, query: &str) {
    let inm = header(&req, "If-None-Match").map(str::to_owned);
    match route {
        Route::Health => respond(req, 200, "text/plain; charset=utf-8", b"ok\n", &[]),
        Route::Index => cached(req, inm, "index", "text/html; charset=utf-8", super::assets::INDEX.as_bytes()),
        Route::Asset => {
            let (path, _) = routes::split_url(req.url());
            match super::assets::find(path) {
                Some(a) => {
                    let (mime, body) = (a.mime, a.body.as_bytes());
                    cached(req, inm, a.path, mime, body)
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
        Route::Icons => cached(req, inm, "icons", "application/json; charset=utf-8", super::icons::json().as_bytes()),
        Route::Snapshot => {
            let json = ctx.web.lock().unwrap_or_else(|e| e.into_inner()).full_json();
            respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[
                ("Cache-Control", "no-store".to_owned()),
            ])
        }
        Route::NotAllowed => respond(req, 405, "text/plain; charset=utf-8", b"method not allowed\n", &[]),
        Route::NotFound => respond(req, 404, "text/plain; charset=utf-8", b"not found\n", &[]),
    }
}

fn cached(
    req: tiny_http::Request,
    if_none_match: Option<String>,
    name: &str,
    mime: &str,
    body: &[u8],
) {
    let tag = super::assets::etag(name);
    if if_none_match.as_deref() == Some(tag.as_str()) {
        return respond(req, 304, mime, b"", &[("ETag", tag)]);
    }
    respond(req, 200, mime, body, &[("ETag", tag)])
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
    use super::*;

    const TOKEN: &str = "test-token-aaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Running {
        _handle: Handle,
        base: String,
    }

    fn serve_test() -> Running {
        let handle = start(
            Config {
                port: 0,
                bind_lan: false,
                token: TOKEN.to_owned(),
                theme: crate::theme::Theme::caldari(),
            },
            crate::web::state::shared(),
        )
        .expect("bind an ephemeral loopback port");
        let base = format!("http://{}", handle.addr);
        Running { _handle: handle, base }
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
        assert_eq!(paired.status(), 302, "a good token is exchanged for a cookie");
        assert_eq!(paired.headers().get("location").unwrap(), "/");
        let cookie = paired.headers().get("set-cookie").unwrap().to_str().unwrap().to_owned();
        assert!(cookie.starts_with(&format!("spai={TOKEN}")));
        assert!(cookie.contains("HttpOnly"), "script must not be able to read the token");
        assert!(cookie.contains("SameSite=Strict"), "this is the CSRF defence");

        let page = c.get(&s.base).header("Cookie", &cookie).send().expect("request");
        assert_eq!(page.status(), 200);
        assert!(page.text().unwrap().contains("EVE Spai"));
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
