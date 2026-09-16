//! The listener.
//!
//! `tiny_http` because it already serves the ESI login callback and the app has no async runtime to
//! borrow. A bind failure disables the feature and says so, like `instance::start_control_listener`.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::routes::{self, Access, Route};
use super::sse::SharedHub;
use super::state::SharedWeb;

const WORKERS: usize = 4;

/// Failed pairing attempts per address per window. The token is 256 bits, so this is log hygiene,
/// and generous so a phone reloading a stale bookmark does not lock itself out.
const MAX_FAILS: u32 = 10;
const FAIL_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub allow_writeback: bool,
    pub bind_lan: bool,
    pub token: String,
    pub theme: crate::theme::Theme,
    /// Overrides `bind_lan`. This and `no_pairing` are advanced options behind a warning.
    pub bind_addr: String,
    pub no_pairing: bool,
    pub map: Option<Arc<super::map::Geometry>>,
}

pub struct Handle {
    server: Arc<tiny_http::Server>,
    running: Arc<AtomicBool>,
    hub: SharedHub,
    /// The address actually bound, for the pairing link the settings pane shows.
    pub addr: String,
}

impl Handle {
    /// Shown in settings so a user can tell whether their phone is connected.
    pub fn clients(&self) -> usize {
        self.hub.client_count()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.server.unblock();
        // Stopping the listener does not end streams it handed out. Close them so pages reconnect
        // to the new listener instead of holding a dead socket.
        self.hub.close_all("restart");
    }
}

struct Ctx {
    cfg: Config,
    web: SharedWeb,
    detail: super::Detail,
    inbox: super::Inbox,
    hub: SharedHub,
    /// Serialized on first request and kept, since the SDE does not change while the app runs.
    map_json: Mutex<Option<Arc<str>>>,
    fails: Mutex<HashMap<IpAddr, (Instant, u32)>>,
}

/// Errs when the port cannot be bound. The caller reports that and leaves the feature off.
pub fn start(
    cfg: Config,
    web: SharedWeb,
    detail: super::Detail,
    inbox: super::Inbox,
) -> Result<Handle, String> {
    let host: &str = if !cfg.bind_addr.trim().is_empty() {
        cfg.bind_addr.trim()
    } else if cfg.bind_lan {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    // Retried: a replaced listener's socket stays bound until its last worker drops the
    // `Arc<Server>`, and workers wake on a 500ms timeout.
    let mut server = None;
    let mut last = String::new();
    for attempt in 0..12 {
        match tiny_http::Server::http((host, cfg.port)) {
            Ok(s) => {
                server = Some(s);
                break;
            }
            Err(e) => {
                last = format!("could not bind {host}:{} ({e})", cfg.port);
                if attempt < 11 {
                    std::thread::sleep(Duration::from_millis(80));
                }
            }
        }
    }
    let server = server.ok_or(last)?;
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
/// Connecting a UDP socket sends no packets but makes the kernel pick the outbound interface, which
/// avoids guessing among enumerated interfaces.
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
    // Unpaired serving is an opt-in behind a warning. It skips the host check too; writes still
    // check `Origin`.
    let access = if ctx.cfg.no_pairing {
        routes::Access::Granted
    } else {
        routes::authorize(
            &route,
            query_token,
            cookie_token,
            host.as_deref(),
            &ctx.cfg.token,
            limited,
        )
    };

    // `Origin` matters only on writes, because a cross-site form post carries the browser's cookie.
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
            // Served directly, not via a 302: a QR scan has no same-site initiator, so the browser
            // would not attach the new cookie to the redirect. The page strips the token from the
            // address bar itself.
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
        Route::Logo => {
            respond(
                req,
                200,
                "image/png",
                super::assets::LOGO,
                &[("Cache-Control", "public, max-age=604800".to_owned())],
            )
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
            let live = ctx.web.lock().unwrap_or_else(|e| e.into_inner()).theme();
            let theme =
                super::css::theme_from_query(query, live.as_ref().unwrap_or(&ctx.cfg.theme));
            respond(req, 200, "text/css; charset=utf-8", super::css::theme_css(&theme).as_bytes(), &[
                ("Cache-Control", "no-store".to_owned()),
            ])
        }
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
        Route::Route => {
            let num = |k: &str| routes::query_param(query, k).and_then(|v| v.parse::<i64>().ok());
            let kind = routes::query_param(query, "kind").unwrap_or("gate").to_owned();
            let d = ctx.detail.lock().unwrap_or_else(|e| e.into_inner());
            let out = match (num("from"), num("to"), d.graph.as_ref()) {
                (Some(from), Some(to), Some(graph)) => {
                    let coords: &[crate::store::MapSystem] =
                        d.coords.as_ref().map(|c| c.as_slice()).unwrap_or(&[]);
                    let skill = |k: &str| {
                        routes::query_param(query, k)
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(5)
                            .min(5)
                    };
                    let (jdc, jfc) = (skill("jdc"), skill("jfc"));
                    let hull = routes::query_param(query, "hull")
                        .and_then(|v| v.parse::<usize>().ok())
                        .filter(|i| *i < crate::jumproute::SHIP_CLASSES.len())
                        .unwrap_or(0);
                    let class = &crate::jumproute::SHIP_CLASSES[hull];
                    // A titan regardless of the hull the jump planner is set to.
                    let titan_ly =
                        crate::jumproute::max_range_ly(&crate::jumproute::SHIP_CLASSES[1], jdc);
                    let mut out = super::route::RouteOut {
                        kind: kind.clone(),
                        from,
                        to,
                        hulls: super::route::hulls(),
                        hull,
                        jdc,
                        jfc,
                        max_ly: crate::jumproute::max_range_ly(class, jdc),
                        via_wormholes: d.via_wormholes,
                        ..Default::default()
                    };
                    let mut anchors = vec![from];
                    anchors.extend(
                        routes::query_param(query, "via")
                            .unwrap_or("")
                            .split(',')
                            .filter_map(|v| v.parse::<i64>().ok()),
                    );
                    anchors.push(to);
                    let ids = |k: &str| -> std::collections::HashSet<i64> {
                        routes::query_param(query, k)
                            .unwrap_or("")
                            .split(',')
                            .filter_map(|v| v.parse::<i64>().ok())
                            .collect()
                    };
                    let avoid = super::route::Avoid {
                        always: if kind == "jump" { &d.avoid_jump } else { &d.avoid_gate }
                            .iter()
                            .copied()
                            .collect(),
                        once: ids("avoid"),
                    };
                    let pick: Vec<usize> = routes::query_param(query, "pick")
                        .unwrap_or("")
                        .split(',')
                        .map(|v| v.parse::<usize>().unwrap_or(0))
                        .collect();
                    let (legs, options) = super::route::chain(
                        graph,
                        coords,
                        &anchors,
                        &kind,
                        class,
                        jdc,
                        jfc,
                        titan_ly,
                        routes::query_param(query, "tstart").unwrap_or("1") != "0",
                        // Per route, not a setting: titan positions belong to the operation being
                        // planned, so the client carries them like the avoid list.
                        &ids("titans").into_iter().collect::<Vec<i64>>(),
                        routes::query_param(query, "tself").unwrap_or("0") == "1",
                        d.count_bridges,
                        &avoid,
                        &d.holes,
                        &pick,
                    );
                    out.avoided = super::route::avoided(graph, &avoid);
                    out.legs = legs;
                    out.options = options;
                    super::route::mark_anchors(&mut out.options, &anchors);
                    {
                        let w = ctx.web.lock().unwrap_or_else(|e| e.into_inner());
                        let danger = super::route::danger_from_marks(
                            &w.intel_marks(),
                            &w.kill_counts(),
                        );
                        drop(w);
                        super::route::annotate(&mut out.options, &danger);
                    }
                    if out.options.is_empty() {
                        out.error = Some(match kind.as_str() {
                            "jump" => "No capital route: every path needs a cyno-able system in range."
                                .to_owned(),
                            "titan" => "Nothing in titan range can reach it by gates.".to_owned(),
                            _ => "No gate route.".to_owned(),
                        });
                    }
                    out
                }
                _ => super::route::RouteOut {
                    kind,
                    error: Some("The star map has not loaded yet.".to_owned()),
                    ..Default::default()
                },
            };
            drop(d);
            let json = serde_json::to_string(&out).unwrap_or_else(|_| "{}".to_owned());
            respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[(
                "Cache-Control",
                "no-store".to_owned(),
            )])
        }
        Route::SavedRoutes => {
            let d = ctx.detail.lock().unwrap_or_else(|e| e.into_inner());
            // Named, so the page can list them without a second round trip per route.
            let named: Vec<serde_json::Value> = d
                .saved_routes
                .iter()
                .map(|r| {
                    let name_of = |id: i64| {
                        d.graph
                            .as_ref()
                            .and_then(|g| g.info_of(id).map(|i| i.name.clone()))
                            .unwrap_or_else(|| id.to_string())
                    };
                    serde_json::json!({
                        "route": r,
                        "from_name": r.anchors.first().copied().map(name_of),
                        "to_name": r.anchors.last().copied().map(name_of),
                    })
                })
                .collect();
            drop(d);
            let json = serde_json::to_string(&named).unwrap_or_else(|_| "[]".to_owned());
            respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[(
                "Cache-Control",
                "no-store".to_owned(),
            )])
        }
        Route::Alternatives => {
            // Picking one inserts a waypoint, the only way to steer a jump route without banning
            // systems.
            let num = |k: &str| routes::query_param(query, k).and_then(|v| v.parse::<i64>().ok());
            let jdc = routes::query_param(query, "jdc")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(5)
                .min(5);
            let hull = routes::query_param(query, "hull")
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|i| *i < crate::jumproute::SHIP_CLASSES.len())
                .unwrap_or(0);
            let d = ctx.detail.lock().unwrap_or_else(|e| e.into_inner());
            let mut out: Vec<serde_json::Value> = Vec::new();
            if let (Some(a), Some(b), Some(graph), Some(coords)) =
                (num("a"), num("b"), d.graph.as_ref(), d.coords.as_ref())
            {
                let max_ly =
                    crate::jumproute::max_range_ly(&crate::jumproute::SHIP_CLASSES[hull], jdc);
                let mut ids = crate::jumproute::alternatives(coords, max_ly, a, b);
                ids.sort_unstable();
                ids.dedup();
                out = ids
                    .into_iter()
                    .filter_map(|id| {
                        let i = graph.info_of(id)?;
                        Some(serde_json::json!({
                            "id": id,
                            "name": i.name,
                            "security": i.security,
                        }))
                    })
                    .collect();
                // The ring between two hops can hold dozens of equivalent answers.
                out.truncate(40);
            }
            drop(d);
            let json = serde_json::to_string(&out).unwrap_or_else(|_| "[]".to_owned());
            respond(req, 200, "application/json; charset=utf-8", json.as_bytes(), &[(
                "Cache-Control",
                "no-store".to_owned(),
            )])
        }
        Route::JabberChat => {
            // The page percent-encodes a JID's `@`.
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
            let body = super::detail::system(id, &d);
            json_or_404(req, body)
        }
        Route::ShipInfo(id) => {
            let d = ctx.detail.lock().unwrap_or_else(|e| e.into_inner());
            // Resolve uncached skill names from ESI rather than showing "Skill 3330". This blocks
            // once per never-seen hull.
            if let (Some(store), Some(cache)) = (d.store.as_ref(), d.type_names.as_ref()) {
                let want = super::detail::ship_skill_ids(id, store);
                let missing: Vec<i64> = {
                    let have = cache.lock().unwrap_or_else(|e| e.into_inner());
                    want.into_iter().filter(|k| !have.contains_key(k)).collect()
                };
                if !missing.is_empty() {
                    let got = crate::universe::lookup_names(&missing);
                    cache.lock().unwrap_or_else(|e| e.into_inner()).extend(got);
                }
            }
            let names = d
                .type_names
                .as_ref()
                .map(|c| c.lock().unwrap_or_else(|e| e.into_inner()).clone())
                .unwrap_or_default();
            let body = d.store.as_ref().and_then(|s| super::detail::ship(id, s, &names));
            json_or_404(req, body)
        }
        Route::NotesExport(id) => {
            let notes = ctx.detail.lock().unwrap_or_else(|e| e.into_inner()).notes.clone();
            let body = notes.export(&id).map(|e| {
                serde_json::json!({
                    "name": e.folder.name,
                    "json": crate::notes::to_json(&e),
                    "compressed": crate::notes::to_compressed(&e),
                })
            });
            json_or_404(req, body)
        }
        Route::Action => unreachable!("answered before `serve`, which cannot read a body"),
        Route::NotFound => respond(req, 404, "text/plain; charset=utf-8", b"not found\n", &[]),
    }
}

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
            if let Some(ui) = ctx.detail.lock().unwrap_or_else(|e| e.into_inner()).wake.clone() {
                ui.request_repaint();
            }
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
    // `no-cache` means revalidate, not "do not store". Without any freshness information a browser
    // may heuristically serve a stale copy without asking.
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

/// `Lax`, because `Strict` withholds the cookie from the first load after a QR scan, which starts
/// outside the browser. Writes are protected by the `Origin` check on `/api/action`.
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
        detail: crate::web::Detail,
    }

    fn serve_test() -> Running {
        let web = crate::web::state::shared();
        let inbox = crate::web::inbox();
        let detail = crate::web::detail();
        let handle = start(
            Config {
                port: 0,
                allow_writeback: true,
                bind_lan: false,
                token: TOKEN.to_owned(),
                theme: crate::theme::Theme::caldari(),
                bind_addr: String::new(),
                no_pairing: false,
                map: None,
            },
            web.clone(),
            detail.clone(),
            inbox.clone(),
        )
        .expect("bind an ephemeral loopback port");
        let base = format!("http://{}", handle.addr);
        Running { _handle: handle, base, web, inbox, detail }
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

    /// A QR scan has no same-site initiator, so a `Strict` cookie on a redirect is withheld and the
    /// device lands on the pair page having just paired.
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
        for path in ["/", "/assets/app.js", "/api/map/geometry", "/api/theme.css"] {
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

    /// An `ETag` without freshness information lets the browser keep a stale copy without asking.
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

    /// The sheet tracks the published theme on the same listener. Restarting the server per colour
    /// change would race the old socket on rebind and could switch the feature off.
    #[test]
    fn the_theme_sheet_follows_the_published_theme_without_a_restart() {
        let s = serve_test();
        let c = client();
        let cookie = format!("spai={TOKEN}");
        let sheet = |c: &reqwest::blocking::Client| {
            c.get(format!("{}/api/theme.css", s.base))
                .header("Cookie", &cookie)
                .send()
                .unwrap()
                .text()
                .unwrap()
        };
        assert!(sheet(&c).contains("--accent: #3fa9c9"), "the config's theme, before anything is published");

        let mut theme = crate::theme::Theme::caldari();
        theme.accent = crate::theme::Rgb::new(0xff, 0x00, 0x99);
        {
            let mut st = s.web.lock().unwrap();
            let rev = st.changed(crate::web::state::Pane::Meta, 1234).unwrap();
            st.put_meta(crate::web::snapshot::Meta {
                rev,
                version: "test",
                theme,
                compact: false,
                intel_ttl_secs: 0,
                intel_max_jumps: 0,
                count_bridges: false,
                allow_writeback: true,
                active_character: String::new(),
                chars: Vec::new(),
                player_system: None,
                sounds: Default::default(),
                sound_rev: 0,
                rescue: false,
                avoid_gate: Vec::new(),
                avoid_jump: Vec::new(),
            });
        }
        assert!(
            sheet(&c).contains("--accent: #ff0099"),
            "the same listener has to serve the new theme"
        );
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

    /// `tiny_http`'s unknown-length response buffers 8 KB without flushing per write, which is why
    /// `sse::serve` frames its own chunks.
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

    /// Pins the exact bodies `notes.js` posts, so a renamed field on either side fails here rather
    /// than as a silent 400 on the page.
    #[test]
    fn the_notes_actions_the_page_sends_are_accepted() {
        use crate::notes::{ImportMode, NotesOp, Subject};
        let s = serve_test();
        let c = client();
        let origin = Some(s.base.as_str());
        let book = crate::uitest::fixtures::notebook();
        let export = crate::notes::to_json(&book.export(&book.folders[0].id).unwrap());
        let bodies = [
            r#"{"Notes":{"SetEntry":{"folder":"f","subject":{"Pilot":{"id":5,"name":"Bob"}},"note":"hi","tags":["d:pilot:cyno"]}}}"#.to_owned(),
            r#"{"Notes":{"SetTag":{"folder":"","subject":{"System":30004759},"tag":"d:sys:staging","on":false}}}"#.to_owned(),
            r#"{"Notes":{"PutTag":{"folder":"f","id":null,"kind":"Pilot","name":"Hunter","color":[239,68,68]}}}"#.to_owned(),
            r#"{"Notes":{"CreateFolder":{"parent":null,"name":"Intel"}}}"#.to_owned(),
            r#"{"Notes":{"SetOnline":{"id":"f","on":false}}}"#.to_owned(),
            r#"{"NotesTarget":{"folder":"f"}}"#.to_owned(),
            format!(r#"{{"Notes":{{"Import":{{"export":{export},"mode":"Merge","parent":null}}}}}}"#),
            r#"{"DefaultTagColor":{"id":"d:pilot:cyno","color":[1,2,3]}}"#.to_owned(),
            r#"{"DefaultTagColor":{"id":"d:pilot:cyno","color":null}}"#.to_owned(),
            r#"{"SetDestination":{"id":30004759}}"#.to_owned(),
            r#"{"SetIngameRoute":{"waypoints":[30004759,30004608]}}"#.to_owned(),
            r#""ClearIngameRoute""#.to_owned(),
        ];
        for b in &bodies {
            assert_eq!(post(&c, &s.base, origin, b).status(), 204, "{b}");
        }
        let q = s.inbox.lock().unwrap();
        assert_eq!(q.len(), bodies.len());
        assert!(matches!(
            &q[0],
            crate::ipc::OverlayToMain::Notes(NotesOp::SetEntry { subject: Subject::Pilot { id: 5, .. }, .. })
        ));
        assert!(matches!(
            &q[6],
            crate::ipc::OverlayToMain::Notes(NotesOp::Import { mode: ImportMode::Merge, .. })
        ));
    }

    #[test]
    fn a_notes_folder_exports_in_both_forms() {
        let s = serve_test();
        let book = crate::uitest::fixtures::notebook();
        let id = book.folders[1].id.clone();
        s.detail.lock().unwrap().notes = std::sync::Arc::new(book.clone());
        let c = client();
        let cookie = format!("spai={TOKEN}");
        let r = c.get(format!("{}/api/notes/export/{id}", s.base)).header("Cookie", &cookie).send().unwrap();
        assert_eq!(r.status(), 200);
        let v: serde_json::Value = r.json().unwrap();
        let want = book.export(&id).unwrap();
        for form in ["json", "compressed"] {
            let text = v[form].as_str().unwrap();
            assert_eq!(crate::notes::parse_export(text).unwrap(), want, "{form}");
        }
        assert_eq!(v["name"], "Coalition intel");
        let r = c
            .get(format!("{}/api/notes/export/00000000-0000-4000-8000-000000000000", s.base))
            .header("Cookie", &cookie)
            .send()
            .unwrap();
        assert_eq!(r.status(), 404);
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
