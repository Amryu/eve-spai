//! EVE Online SSO login via OAuth2 **PKCE** (public client, no secret), with a
//! loopback callback server (docs/DESIGN.md §7.1 E2). The client ID is
//! configurable in Settings and defaults to the project's registered application.
//!
//! Flow: build the authorize URL with a PKCE challenge → open the browser → catch
//! the redirect on a local HTTP server → exchange the code for tokens → decode the
//! access-token JWT claims for the character id/name → persist the character.
//!
//! NOTE (scaffold): tokens are stored in plaintext in the local SQLite DB and the
//! JWT signature is not yet verified against EVE's JWKS — the token is trusted
//! because it came directly from the SSO token endpoint over TLS. Hardening
//! (OS keychain at rest, JWKS verification) is tracked as D5 in the design doc.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context as _, Result};
use base64::Engine;
use rusqlite::{params, Connection};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const AUTHORIZE_URL: &str = "https://login.eveonline.com/v2/oauth/authorize/";
const TOKEN_URL: &str = "https://login.eveonline.com/v2/oauth/token";

/// The project's registered EVE application (a public PKCE client).
pub const DEFAULT_CLIENT_ID: &str = "fef96bde615b450bba89c9414962ca38";
pub const DEFAULT_CALLBACK: &str = "http://localhost:8765/callback";

/// Minimal scope set for M1 (location + contacts for standings). More scopes are
/// requested per-feature as Advanced views are built (docs/DESIGN.md §7.1 E2).
pub const DEFAULT_SCOPES: &[&str] = &[
    "esi-location.read_location.v1",
    "esi-location.read_online.v1",
    "esi-location.read_ship_type.v1",
    "esi-characters.read_contacts.v1",
];

/// Observable state of a login attempt, shared with the UI.
#[derive(Clone, Debug, Default)]
pub enum AuthStatus {
    #[default]
    Idle,
    Waiting(String),
    Success(String),
    Failed(String),
}

pub type SharedAuth = Arc<Mutex<AuthStatus>>;

/// Start an interactive login on a background thread.
pub fn spawn_login(
    client_id: String,
    callback: String,
    scopes: Vec<String>,
    db_path: PathBuf,
    status: SharedAuth,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let set = |s: AuthStatus| {
            *status.lock().unwrap() = s;
            ctx.request_repaint();
        };
        set(AuthStatus::Waiting("Opening browser…".to_owned()));
        match run(&client_id, &callback, &scopes, &db_path, &set) {
            Ok(name) => set(AuthStatus::Success(name)),
            Err(e) => set(AuthStatus::Failed(format!("{e:#}"))),
        }
    });
}

fn run(
    client_id: &str,
    callback: &str,
    scopes: &[String],
    db_path: &PathBuf,
    set: &impl Fn(AuthStatus),
) -> Result<String> {
    let verifier = b64url(&random_bytes(32)?);
    let challenge = b64url(Sha256::digest(verifier.as_bytes()).as_slice());
    let state = b64url(&random_bytes(16)?);
    let scope = scopes.join(" ");

    let auth_url = reqwest::Url::parse_with_params(
        AUTHORIZE_URL,
        &[
            ("response_type", "code"),
            ("redirect_uri", callback),
            ("client_id", client_id),
            ("scope", scope.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
        ],
    )?;

    // Bind the callback server before opening the browser so we don't miss the hit.
    let port = reqwest::Url::parse(callback)?
        .port()
        .ok_or_else(|| anyhow!("callback URL must include a port"))?;
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow!("could not bind callback server on port {port}: {e}"))?;

    let _ = open::that(auth_url.as_str());
    set(AuthStatus::Waiting(
        "Waiting for EVE login in your browser…".to_owned(),
    ));

    let (code, got_state) = wait_for_callback(&server)?;
    if got_state != state {
        bail!("state mismatch — aborting (possible CSRF)");
    }

    set(AuthStatus::Waiting("Exchanging authorization code…".to_owned()));
    let token = exchange_code(client_id, &code, &verifier)?;
    let claims = decode_claims(&token.access_token)?;
    let id = claims.character_id()?;
    let expires_at = chrono::Utc::now().timestamp() + token.expires_in;

    store_character(
        db_path,
        id,
        &claims.name,
        &token.refresh_token,
        &token.access_token,
        expires_at,
        &scope,
    )?;
    Ok(claims.name)
}

/// Block until the OAuth redirect arrives (or time out), answering the browser.
fn wait_for_callback(server: &tiny_http::Server) -> Result<(String, String)> {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| anyhow!("login timed out"))?;
        let request = match server.recv_timeout(remaining)? {
            Some(r) => r,
            None => bail!("login timed out"),
        };

        // Parse query params from the request path.
        let full = format!("http://localhost{}", request.url());
        let parsed = reqwest::Url::parse(&full)?;
        let mut code = None;
        let mut state = None;
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "code" => code = Some(v.into_owned()),
                "state" => state = Some(v.into_owned()),
                _ => {}
            }
        }

        let body = "<!doctype html><html><body style=\"font-family:system-ui;background:#0b0f12;\
            color:#c8d2d8;display:flex;height:100vh;align-items:center;justify-content:center\">\
            <div><h2>EVE Spai</h2><p>Login complete — you can close this tab.</p></div></body></html>";
        let header =
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                .unwrap();
        let _ = request.respond(tiny_http::Response::from_string(body).with_header(header));

        if let (Some(c), Some(s)) = (code, state) {
            return Ok((c, s));
        }
        // Ignore stray requests (e.g. favicon) and keep waiting.
    }
}

#[derive(Deserialize)]
pub struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

fn exchange_code(client_id: &str, code: &str, verifier: &str) -> Result<TokenResponse> {
    let client = http_client()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client_id),
            ("code_verifier", verifier),
        ])
        .send()?;
    if !resp.status().is_success() {
        let status = resp.status();
        bail!(
            "token endpoint returned {status}: {}",
            resp.text().unwrap_or_default()
        );
    }
    resp.json().context("parsing token response")
}

/// Exchange a stored refresh token for a fresh access token (used by ESI calls in
/// later milestones).
#[allow(dead_code)]
pub fn refresh_access_token(client_id: &str, refresh_token: &str) -> Result<TokenResponse> {
    let client = http_client()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .send()?;
    if !resp.status().is_success() {
        let status = resp.status();
        bail!(
            "refresh returned {status}: {}",
            resp.text().unwrap_or_default()
        );
    }
    resp.json().context("parsing refresh response")
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent("eve-spai/0.1 (EVE intel tool)")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(Into::into)
}

#[derive(Deserialize)]
struct Claims {
    /// e.g. "CHARACTER:EVE:2112000000"
    sub: String,
    name: String,
}

impl Claims {
    fn character_id(&self) -> Result<i64> {
        self.sub
            .rsplit(':')
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("unexpected sub claim: {}", self.sub))
    }
}

/// Decode (without signature verification — see module note) the JWT payload.
fn decode_claims(jwt: &str) -> Result<Claims> {
    let payload = jwt.split('.').nth(1).ok_or_else(|| anyhow!("malformed JWT"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .context("decoding JWT payload")?;
    serde_json::from_slice(&bytes).context("parsing JWT claims")
}

fn store_character(
    path: &PathBuf,
    id: i64,
    name: &str,
    refresh_token: &str,
    access_token: &str,
    expires_at: i64,
    scopes: &str,
) -> Result<()> {
    // Secrets to the keychain first — if that fails we abort before persisting
    // anything, so we never silently fall back to plaintext.
    crate::tokens::save(
        id,
        &crate::tokens::Tokens {
            refresh_token: refresh_token.to_owned(),
            access_token: access_token.to_owned(),
        },
    )?;

    // Only non-secret metadata goes in SQLite.
    let conn = Connection::open(path)?;
    conn.execute(
        "INSERT INTO characters (id, name, expires_at, scopes)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET name = ?2, expires_at = ?3, scopes = ?4",
        params![id, name, expires_at, scopes],
    )?;
    Ok(())
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random_bytes(n: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    getrandom::getrandom(&mut buf).map_err(|e| anyhow!("rng failure: {e}"))?;
    Ok(buf)
}
