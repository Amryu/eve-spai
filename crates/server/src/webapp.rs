//! The web wormhole map at `/wh/`: a trunk build, loaded from `WH_WEB_DIR` at start. Any path
//! under `/wh/` that is no file gets the app's page, which routes itself (join links, the SSO
//! callback). The page's inline script and style are allowed by hash, nothing else inline.

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use base64::Engine as _;
use sha2::Digest as _;

struct File {
    body: Vec<u8>,
    gz: Option<Vec<u8>>,
    kind: &'static str,
    /// Named by its content hash, so a new build is a new name and this one never changes.
    immutable: bool,
}

pub struct WebApp {
    files: HashMap<String, File>,
    csp: String,
}

fn kind(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("gz") => "application/gzip",
        Some("json") => "application/json",
        Some("css") => "text/css; charset=utf-8",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// trunk names what it builds `<crate>-<16 hex>.js` and `…_bg.wasm`.
fn content_named(name: &str) -> bool {
    let stem = name.trim_end_matches("_bg.wasm").trim_end_matches(".js");
    stem.len() != name.len() && stem.rsplit('-').next().is_some_and(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Every `<tag …>body</tag>` in `html`, bodies only.
fn inline_blocks<'a>(html: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i..];
        let Some(gt) = after.find('>') else { break };
        let body_start = &after[gt + 1..];
        let Some(end) = body_start.find(&close) else { break };
        // `<script src=…>` and `<style>` both count; an empty body is a reference, not inline code.
        if !body_start[..end].trim().is_empty() {
            out.push(&body_start[..end]);
        }
        rest = &body_start[end + close.len()..];
    }
    out
}

fn hashes(blocks: &[&str]) -> String {
    blocks
        .iter()
        .map(|b| format!("'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(b.as_bytes()))))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Only this origin, the EVE sign-in and ESI; WebAssembly compiled from our own file.
pub fn csp(index: &str) -> String {
    format!(
        "default-src 'self'; script-src 'self' 'wasm-unsafe-eval' {}; style-src 'self' {}; \
         connect-src 'self' https://login.eveonline.com https://esi.evetech.net https://api.eve-scout.com; img-src 'self' data: https://images.evetech.net; \
         object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        hashes(&inline_blocks(index, "script")),
        hashes(&inline_blocks(index, "style")),
    )
}

impl WebApp {
    /// The build in `dir`, or `None` when there is none there.
    pub fn load(dir: &std::path::Path) -> anyhow::Result<Option<Self>> {
        let index = dir.join("index.html");
        if !index.exists() {
            return Ok(None);
        }
        let mut files = HashMap::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let body = std::fs::read(entry.path())?;
            let k = kind(&name);
            // The wasm and its glue are most of the download and compress well; the universe
            // file is gzip already.
            let gz = matches!(k, "application/wasm" | "text/javascript; charset=utf-8" | "text/html; charset=utf-8")
                .then(|| {
                    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
                    e.write_all(&body).ok()?;
                    e.finish().ok()
                })
                .flatten();
            files.insert(name.clone(), File { immutable: content_named(&name), body, gz, kind: k });
        }
        let csp = csp(&String::from_utf8_lossy(&files["index.html"].body));
        Ok(Some(WebApp { files, csp }))
    }
}

pub fn routes(app: Option<Arc<WebApp>>) -> Router {
    let Some(app) = app else { return Router::new() };
    Router::new()
        .route("/wh", get(|| async { Redirect::permanent("/wh/") }))
        .route("/wh/", get(|h: HeaderMap, State(app): State<Arc<WebApp>>| async move { serve(&app, "", &h) }))
        .route("/wh/{*path}", get(|Path(p): Path<String>, h: HeaderMap, State(app): State<Arc<WebApp>>| async move { serve(&app, &p, &h) }))
        .with_state(app)
}

fn serve(app: &WebApp, path: &str, req: &HeaderMap) -> Response {
    // A file only by its plain name; anything else (join links, the callback) is the app's page.
    let name = if !path.contains('/') && app.files.contains_key(path) { path } else { "index.html" };
    let f = &app.files[name];
    let gzip_ok = req.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("gzip"));
    let (body, gzipped) = match (&f.gz, gzip_ok) {
        (Some(gz), true) => (gz.clone(), true),
        _ => (f.body.clone(), false),
    };
    let mut resp = (StatusCode::OK, body).into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(f.kind));
    if gzipped {
        h.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    }
    h.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(if f.immutable { "public, max-age=31536000, immutable" } else { "no-cache" }),
    );
    if let Ok(v) = HeaderValue::from_str(&app.csp) {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt as _;

    const INDEX: &str = "<html><head><script type=\"module\">import init from '/wh/spai-web-0123456789abcdef.js';</script>\
        <style>body { margin: 0 }</style><link rel=\"modulepreload\" href=\"/wh/x.js\"></head><body></body></html>";

    fn build() -> tempdir::Dir {
        let d = tempdir::Dir::new();
        std::fs::write(d.0.join("index.html"), INDEX).unwrap();
        std::fs::write(d.0.join("spai-web-0123456789abcdef.js"), "export default 1;".repeat(100)).unwrap();
        std::fs::write(d.0.join("universe.json.gz"), [0x1f, 0x8b, 1, 2]).unwrap();
        d
    }

    mod tempdir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                let p = std::env::temp_dir().join(format!("spai-webapp-{}-{:?}", std::process::id(), std::thread::current().id()));
                let _ = std::fs::remove_dir_all(&p);
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    async fn get(app: &Router, path: &str) -> Response {
        app.clone().oneshot(Request::get(path).header("accept-encoding", "gzip").body(Body::empty()).unwrap()).await.unwrap()
    }

    #[test]
    fn the_policy_allows_exactly_the_pages_inline_blocks() {
        let p = csp(INDEX);
        let sha = |s: &str| format!("'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(s.as_bytes())));
        assert!(p.contains(&sha("import init from '/wh/spai-web-0123456789abcdef.js';")), "{p}");
        assert!(p.contains(&sha("body { margin: 0 }")), "{p}");
        assert!(!p.contains("unsafe-inline") && p.contains("'wasm-unsafe-eval'") && p.contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn files_are_served_and_every_other_path_is_the_page() {
        let d = build();
        let app = routes(WebApp::load(&d.0).unwrap().map(Arc::new));
        let js = get(&app, "/wh/spai-web-0123456789abcdef.js").await;
        assert_eq!(js.headers()[header::CONTENT_ENCODING], "gzip");
        assert!(js.headers()[header::CACHE_CONTROL].to_str().unwrap().contains("immutable"));
        let join = get(&app, "/wh/join/abc123").await;
        assert_eq!(join.status(), StatusCode::OK);
        assert_eq!(join.headers()[header::CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(join.headers()[header::CACHE_CONTROL], "no-cache");
        assert!(join.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("sha256-"));
        let uni = get(&app, "/wh/universe.json.gz").await;
        assert_eq!((uni.headers()[header::CONTENT_TYPE].to_str().unwrap(), uni.headers().get(header::CONTENT_ENCODING)), ("application/gzip", None));
        assert_eq!(get(&app, "/wh/../Cargo.toml").await.headers()[header::CONTENT_TYPE], "text/html; charset=utf-8");
        assert_eq!(get(&app, "/wh").await.status(), StatusCode::PERMANENT_REDIRECT);
    }

    #[test]
    fn no_build_no_routes() {
        let d = tempdir::Dir::new();
        assert!(WebApp::load(&d.0).unwrap().is_none());
    }
}
