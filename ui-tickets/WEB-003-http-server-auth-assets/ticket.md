# WEB-003 &mdash; The app serves no page, and binding a LAN socket has no access control

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `web/{server,routes,css,icons}.rs`, `assets/{index.html,app.css,app.js}` |
| **Reported by** | user, remote web view |

## Gap

There is no HTTP server in the app beyond the one-shot ESI SSO callback (`app/src/auth.rs:94`), and
that one binds loopback for 180 seconds and exits. This feature binds `0.0.0.0` for as long as the app
runs, which needs routing, an asset pipeline and a real access story.

## Deliverable

A `tiny_http` server started from `SpaiApp::build` beside `instance::start_control_listener`
(`app.rs:802`), behind the same `if !headless` guard, restarted when the port or bind setting changes.
A few workers over `server.recv()`.

Auth on every request: `?t=<token>` validates and returns `302` plus
`Set-Cookie: spai=<token>; Path=/; Max-Age=31536000; HttpOnly; SameSite=Strict`, otherwise the cookie
is checked, otherwise `403` and a small "pair this device" page. `Host` must parse as an IP literal
via `IpAddr::from_str` or be `localhost`. `Origin` is checked the same way on POSTs. Ten failed token
attempts per IP per minute returns `429`.

Assets from a `const ASSETS: &[(path, mime, body)]` table of `include_str!`s, each with
`ETag: W/"<version>-<name>"` and `If-None-Match` honoured. `/api/theme.css` emitted from
`theme::derived()` plus `theme::standing::*`, `severity_color` (`app.rs:22920`) and the eleven
`security_color` stops (`app.rs:24693`), with `?bg=&fg=&accent=` running the same derivation for a
per-device override. The phosphor TTF from `egui_phosphor::Variant::Regular.font_bytes()` at
`/assets/phosphor-<version>.ttf`, immutable. `/api/icons.json` emitting the name to codepoint mapping
from the Rust constants.

A shell page that loads, themes itself and says it is connected, so the ticket stands alone.

## Notes

`SameSite=Strict` is the CSRF defence. No `Secure`, there is no TLS.

The `Host` check is what stops DNS rebinding, which is the real risk of a `0.0.0.0` bind. Parse it;
`127.0.0.1.evil.com` passes a `starts_with` check and must not pass this one.

Bind failure degrades exactly as `instance::start_control_listener` does: log it, disable the feature,
never fail the app. The settings pane surfaces it in WEB-012.

Serving the phosphor font the app already links costs no binary size and guarantees the icons match.
Do not hand-author SVG for forty icons; CLAUDE.md's own lesson is that a wrong glyph renders as tofu.

`index.html` here is the shell only. WEB-005 adds the empty pane slots, so WEB-006 and WEB-007 never
edit the same file.

## How to verify

- Table-driven tests on pure `route(method, path, has_cookie)`, `host_allowed` and `origin_allowed`,
  covering `localhost`, `127.0.0.1`, `192.168.1.5:6767`, `[::1]`, `evil.com`, `127.0.0.1.evil.com` and
  a missing header.
- An integration test binding `127.0.0.1:0`: no token 403, wrong token 403, right token 302 with the
  cookie set, cookie alone 200, `If-None-Match` 304, eleventh bad token 429.
- A fix would be WRONG if it compared the token with `==` on `String`, or if it accepted a `Host` it
  only prefix-matched.
