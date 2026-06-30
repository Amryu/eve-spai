//! Battle-report sharing to the eve-spai.com server (docs/DESIGN.md §7.2, Milestone 5).
//!
//! Wraps the server contract: gzip a [`BattleReportDoc`] and `POST /api/br`, list the
//! caller's reports with `GET /api/br/mine`, and `DELETE /api/br/{id}`. The active
//! character's EVE SSO access token (refreshed on demand via
//! [`crate::auth::refresh_access_token`]) is presented ONCE to `POST /api/session` to mint a
//! short-lived server **session token**; that session token — cached per character for its
//! lifetime — is the `Bearer` on every battle-report call. The EVE token never leaves
//! [`mint_session`].
//!
//! Network work runs on a spawned thread (the `spawn_*` helpers); results are published to
//! the UI through `Arc<Mutex<…>>` state plus `ctx.request_repaint()`, mirroring the SSO
//! login flow in [`crate::auth`]. The same threading + repaint pattern as `zkill::spawn`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use br_core::battle::BattleReportDoc;
use crate::auth::{self, DEFAULT_CLIENT_ID};

/// Default sharing server. Overridable at runtime via `EVE_SPAI_API_BASE` so the app can be
/// pointed at a local server during testing.
pub const BR_API_BASE: &str = "https://eve-spai.com";

/// The sharing-server base URL (env override, else [`BR_API_BASE`]). Trailing slashes are
/// trimmed so `{base}/api/br` is always well-formed.
pub fn api_base() -> String {
    let raw = std::env::var("EVE_SPAI_API_BASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| BR_API_BASE.to_string());
    raw.trim_end_matches('/').to_string()
}

/// Refresh the access token when it expires within this many seconds (clock-skew margin).
const REFRESH_SKEW_SECS: i64 = 60;

// --- Wire shapes (mirror crates/server/src/models.rs) ----------------------------------

/// `POST /api/br` success body.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateResponse {
    pub id: String,
    pub url: String,
}

/// One row of `GET /api/br/mine` (owner view). `started_at` is kept as the server's RFC3339
/// string (chrono's serde feature isn't enabled here); the UI parses it only for display.
#[derive(Debug, Clone, Deserialize)]
pub struct ReportRow {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub systems: Vec<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub kills: i64,
    #[serde(default)]
    pub total_isk: f64,
    #[serde(default)]
    pub views: i64,
    #[serde(default)]
    pub unlisted: Option<bool>,
    /// Some server builds include the canonical url; otherwise derive it from the id.
    #[serde(default)]
    pub url: Option<String>,
}

impl ReportRow {
    /// The public URL for this report (server-provided, else derived `{base}/br/{id}`).
    pub fn url(&self, base: &str) -> String {
        match self.url.as_deref().filter(|u| !u.is_empty()) {
            Some(u) => u.to_string(),
            None => format!("{base}/br/{}", self.id),
        }
    }
}

/// `GET /api/br/mine` returns a `ReportPage { page, per_page, reports }` (not a bare array).
#[derive(Debug, Deserialize)]
struct ReportPage {
    #[serde(default)]
    reports: Vec<ReportRow>,
}

// --- Bearer-token resolution -----------------------------------------------------------

/// What to do for a character's bearer token, decided from stored state (pure, so it can be
/// unit-tested without a keychain or network).
#[derive(Debug, PartialEq)]
enum BearerDecision {
    /// The cached access token is still fresh — reuse it.
    Reuse(String),
    /// The access token is missing/expired but a refresh token is available — refresh.
    Refresh(String),
    /// No usable credentials — the UI should prompt a login.
    None,
}

/// Decide how to obtain a bearer token from the cached access token, the refresh token, and
/// the access token's expiry. Refreshes when within [`REFRESH_SKEW_SECS`] of expiry.
fn decide_bearer(
    access: Option<String>,
    refresh: Option<String>,
    expires_at: i64,
    now: i64,
) -> BearerDecision {
    let access = access.filter(|a| !a.is_empty());
    if let Some(a) = &access {
        if expires_at > now + REFRESH_SKEW_SECS {
            return BearerDecision::Reuse(a.clone());
        }
    }
    match refresh.filter(|r| !r.is_empty()) {
        Some(r) => BearerDecision::Refresh(r),
        None => BearerDecision::None,
    }
}

/// The active character's valid access token, refreshing + re-persisting if the cached one is
/// expired (or within ~60s of it). Returns `None` when there is no usable token, so the UI can
/// prompt a login. Opens its own DB connection (callable from a worker thread).
pub fn valid_bearer(store_path: &Path, char_id: i64) -> Option<String> {
    use rusqlite::{params, Connection};

    let conn = Connection::open(store_path).ok()?;
    crate::store::apply_pragmas(&conn);
    let access: Option<String> = conn
        .query_row(
            "SELECT value FROM kv WHERE key = ?1",
            params![format!("access:{char_id}")],
            |r| r.get(0),
        )
        .ok();
    let expires_at: i64 = conn
        .query_row(
            "SELECT COALESCE(expires_at, 0) FROM characters WHERE id = ?1",
            params![char_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let refresh = crate::tokens::load_refresh(char_id);
    let now = chrono::Utc::now().timestamp();

    match decide_bearer(access, refresh, expires_at, now) {
        BearerDecision::Reuse(a) => Some(a),
        BearerDecision::Refresh(r) => {
            let tok = auth::refresh_access_token(DEFAULT_CLIENT_ID, &r).ok()?;
            let new_expiry = now + tok.expires_in;
            // Re-persist exactly like the login flow: cached access token in kv, expiry on the
            // character row, and the (possibly rotated) refresh token back to the keychain.
            let _ = conn.execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![format!("access:{char_id}"), tok.access_token],
            );
            let _ = conn.execute(
                "UPDATE characters SET expires_at = ?1 WHERE id = ?2",
                params![new_expiry, char_id],
            );
            let _ = crate::tokens::save_refresh(char_id, &tok.refresh_token);
            Some(tok.access_token)
        }
        BearerDecision::None => None,
    }
}

// --- Server session tokens -------------------------------------------------------------

/// Re-mint a session when it expires within this many seconds (clock-skew margin).
const SESSION_SKEW_SECS: i64 = 60;

/// A server session minted by `POST /api/session` from an EVE access token. The `token` is
/// the `Bearer` for all battle-report calls; the EVE token is never sent again.
#[derive(Debug, Clone, Deserialize)]
pub struct Session {
    pub token: String,
    pub expires_at: i64,
    pub character_id: i64,
    pub character_name: String,
}

/// Process-lifetime cache of the minted session, keyed by character id. Re-minting on a
/// character switch is automatic — each id has its own entry.
fn session_cache() -> &'static Mutex<std::collections::HashMap<i64, Session>> {
    static CACHE: std::sync::OnceLock<Mutex<std::collections::HashMap<i64, Session>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// What to do for a character's session token, decided from the cached session (pure, so it
/// can be unit-tested without network).
#[derive(Debug, PartialEq)]
enum SessionAction {
    /// The cached session is still comfortably valid — reuse its token.
    Reuse(String),
    /// No cached session, or it is expired / within [`SESSION_SKEW_SECS`] of expiry — mint.
    Mint,
}

/// Decide whether a cached session can be reused or must be re-minted. Mints when the cache
/// is empty or the session expires within [`SESSION_SKEW_SECS`].
fn session_action(cached: Option<&Session>, now: i64) -> SessionAction {
    match cached {
        Some(s) if s.expires_at > now + SESSION_SKEW_SECS => SessionAction::Reuse(s.token.clone()),
        _ => SessionAction::Mint,
    }
}

/// `POST {api_base}/api/session` — exchange the EVE access token for a server session token.
/// The EVE token is the `Bearer` here and only here. 401 → [`ShareError::NeedLogin`].
pub fn mint_session(eve_bearer: &str) -> Result<Session, ShareError> {
    let client = http_client().map_err(|e| ShareError::Network(e.to_string()))?;
    let resp = client
        .post(format!("{}/api/session", api_base()))
        .bearer_auth(eve_bearer)
        .send()
        .map_err(|e| ShareError::Network(e.to_string()))?;
    let status = resp.status();
    if status.is_success() {
        return resp.json::<Session>().map_err(|e| ShareError::Network(e.to_string()));
    }
    Err(status_to_error(status))
}

/// The active character's valid server **session token**, the `Bearer` for all battle-report
/// calls. Reuses the cached session while it is fresh; otherwise obtains a fresh EVE token via
/// [`valid_bearer`], mints a session via [`mint_session`], caches it, and returns its token.
/// Returns `None` when there is no usable EVE credential (UI prompts a login) or minting fails.
/// Opens its own DB connection (callable from a worker thread).
pub fn valid_session(store_path: &Path, char_id: i64) -> Option<String> {
    let now = chrono::Utc::now().timestamp();
    // Fast path: a still-fresh cached session needs no EVE token and no network.
    if let SessionAction::Reuse(token) =
        session_action(session_cache().lock().unwrap().get(&char_id), now)
    {
        return Some(token);
    }
    // Mint: present the EVE token once, cache the session, hand back its token.
    let eve_bearer = valid_bearer(store_path, char_id)?;
    let session = mint_session(&eve_bearer).ok()?;
    if session.character_id != char_id {
        // The server minted a session for a different character than the EVE token we sent —
        // don't cache or use it under this id. Surface it; the UI will re-prompt.
        eprintln!(
            "[brshare] session character mismatch: requested {char_id}, server returned {} ({})",
            session.character_id, session.character_name
        );
        return None;
    }
    let token = session.token.clone();
    session_cache().lock().unwrap().insert(char_id, session);
    Some(token)
}

// --- HTTP --------------------------------------------------------------------------------

fn http_client() -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
        .timeout(Duration::from_secs(45))
        .build()
}

/// gzip a battle-report document's JSON (the request body for `POST /api/br`).
pub fn gzip_json(doc: &BattleReportDoc) -> std::io::Result<Vec<u8>> {
    let json = serde_json::to_vec(doc)?;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&json)?;
    enc.finish()
}

/// A share/upload failure, mapped to a clear user-facing message.
#[derive(Debug)]
pub enum ShareError {
    /// 401 — the token is bad/expired and could not be refreshed.
    NeedLogin,
    /// 413 — the compressed report exceeds the server cap.
    TooLarge,
    /// 429 — upload quota reached.
    Quota,
    /// 400/415 — the server rejected the report.
    Rejected,
    /// Transport error or unexpected status.
    Network(String),
}

impl ShareError {
    pub fn message(&self) -> String {
        match self {
            ShareError::NeedLogin => {
                "Login expired — re-authenticate on the Characters page, then try again.".into()
            }
            ShareError::TooLarge => "Report too large to share (over the server's size limit).".into(),
            ShareError::Quota => "Sharing quota reached — try again later.".into(),
            ShareError::Rejected => "The server rejected this report.".into(),
            ShareError::Network(e) => format!("Could not reach eve-spai.com: {e}"),
        }
    }
}

fn status_to_error(status: reqwest::StatusCode) -> ShareError {
    match status.as_u16() {
        401 => ShareError::NeedLogin,
        413 => ShareError::TooLarge,
        429 => ShareError::Quota,
        400 | 415 => ShareError::Rejected,
        _ => ShareError::Network(format!("server returned {status}")),
    }
}

/// `POST /api/br?unlisted=…` — gzip + Bearer upload of a battle report.
pub fn upload(
    base: &str,
    bearer: &str,
    doc: &BattleReportDoc,
    unlisted: bool,
) -> Result<CreateResponse, ShareError> {
    let body = gzip_json(doc).map_err(|e| ShareError::Network(e.to_string()))?;
    let client = http_client().map_err(|e| ShareError::Network(e.to_string()))?;
    let resp = client
        .post(format!("{base}/api/br?unlisted={unlisted}"))
        .bearer_auth(bearer)
        .header(reqwest::header::CONTENT_ENCODING, "gzip")
        .body(body)
        .send()
        .map_err(|e| ShareError::Network(e.to_string()))?;
    let status = resp.status();
    if status.is_success() {
        return resp.json::<CreateResponse>().map_err(|e| ShareError::Network(e.to_string()));
    }
    Err(status_to_error(status))
}

/// `GET /api/br/mine` — the caller's reports (newest first, incl. unlisted).
pub fn fetch_mine(base: &str, bearer: &str) -> Result<Vec<ReportRow>, ShareError> {
    let client = http_client().map_err(|e| ShareError::Network(e.to_string()))?;
    let resp = client
        .get(format!("{base}/api/br/mine"))
        .bearer_auth(bearer)
        .send()
        .map_err(|e| ShareError::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(status_to_error(status));
    }
    let page = resp.json::<ReportPage>().map_err(|e| ShareError::Network(e.to_string()))?;
    Ok(page.reports)
}

/// Outcome of a `DELETE /api/br/{id}`.
#[derive(Debug)]
pub enum DeleteOutcome {
    /// 204/200 — gone (or already gone).
    Deleted,
    /// 403 — the caller doesn't own the report.
    NotOwner,
    /// 401 — token bad/expired.
    NeedLogin,
    /// Transport error or unexpected status.
    Error(String),
}

impl DeleteOutcome {
    /// User-facing message for a non-success outcome (`Deleted` has none).
    pub fn message(&self) -> Option<String> {
        match self {
            DeleteOutcome::Deleted => None,
            DeleteOutcome::NotOwner => Some("You are not the owner of that report.".into()),
            DeleteOutcome::NeedLogin => {
                Some("Login expired — re-authenticate on the Characters page.".into())
            }
            DeleteOutcome::Error(e) => Some(format!("Could not delete: {e}")),
        }
    }
}

/// `DELETE /api/br/{id}` — owner-only. A 404 (already missing) counts as `Deleted` so the row
/// disappears either way.
pub fn delete(base: &str, bearer: &str, id: &str) -> DeleteOutcome {
    let client = match http_client() {
        Ok(c) => c,
        Err(e) => return DeleteOutcome::Error(e.to_string()),
    };
    let resp = match client.delete(format!("{base}/api/br/{id}")).bearer_auth(bearer).send() {
        Ok(r) => r,
        Err(e) => return DeleteOutcome::Error(e.to_string()),
    };
    match resp.status().as_u16() {
        200 | 204 | 404 => DeleteOutcome::Deleted,
        401 => DeleteOutcome::NeedLogin,
        403 => DeleteOutcome::NotOwner,
        s => DeleteOutcome::Error(format!("server returned {s}")),
    }
}

// --- UI-facing async state + spawners ---------------------------------------------------

/// State of a share (upload) action, shared with the UI thread.
#[derive(Default)]
pub enum ShareStatus {
    #[default]
    Idle,
    Uploading,
    Done {
        id: String,
        url: String,
    },
    Error(String),
}

pub type SharedShare = Arc<Mutex<ShareStatus>>;

/// State of the "My shared BRs" panel, shared with the UI thread.
#[derive(Default)]
pub struct MineState {
    pub status: MineStatus,
    /// Transient feedback (e.g. a delete failure) shown above the list.
    pub msg: Option<String>,
}

#[derive(Default)]
pub enum MineStatus {
    #[default]
    Idle,
    Loading,
    Loaded(Vec<ReportRow>),
    Error(String),
}

pub type SharedMine = Arc<Mutex<MineState>>;

/// Upload `doc` on a background thread, publishing the result to `state`.
pub fn spawn_share(
    doc: BattleReportDoc,
    store_path: PathBuf,
    char_id: i64,
    unlisted: bool,
    state: SharedShare,
    ctx: egui::Context,
) {
    *state.lock().unwrap() = ShareStatus::Uploading;
    ctx.request_repaint();
    std::thread::spawn(move || {
        let result = match valid_session(&store_path, char_id) {
            None => Err(ShareError::NeedLogin),
            Some(bearer) => upload(&api_base(), &bearer, &doc, unlisted),
        };
        *state.lock().unwrap() = match result {
            Ok(r) => ShareStatus::Done { id: r.id, url: r.url },
            Err(e) => ShareStatus::Error(e.message()),
        };
        ctx.request_repaint();
    });
}

/// Load the caller's reports on a background thread into `state`.
pub fn spawn_load_mine(store_path: PathBuf, char_id: i64, state: SharedMine, ctx: egui::Context) {
    {
        let mut s = state.lock().unwrap();
        s.status = MineStatus::Loading;
        s.msg = None;
    }
    ctx.request_repaint();
    std::thread::spawn(move || {
        let result = match valid_session(&store_path, char_id) {
            None => Err(ShareError::NeedLogin),
            Some(bearer) => fetch_mine(&api_base(), &bearer),
        };
        let mut s = state.lock().unwrap();
        match result {
            Ok(rows) => {
                s.status = MineStatus::Loaded(rows);
                s.msg = None;
            }
            Err(e) => s.status = MineStatus::Error(e.message()),
        }
        drop(s);
        ctx.request_repaint();
    });
}

/// Delete a report from the "My shared BRs" panel; on success reload the list, otherwise set
/// `state.msg`.
pub fn spawn_delete_mine(
    store_path: PathBuf,
    char_id: i64,
    id: String,
    state: SharedMine,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let base = api_base();
        let outcome = match valid_session(&store_path, char_id) {
            None => DeleteOutcome::NeedLogin,
            Some(bearer) => delete(&base, &bearer, &id),
        };
        match outcome {
            DeleteOutcome::Deleted => {
                // Re-fetch so the list reflects the deletion (and any concurrent changes).
                let reloaded = valid_session(&store_path, char_id)
                    .ok_or(ShareError::NeedLogin)
                    .and_then(|b| fetch_mine(&base, &b));
                let mut s = state.lock().unwrap();
                match reloaded {
                    Ok(rows) => {
                        s.status = MineStatus::Loaded(rows);
                        s.msg = None;
                    }
                    Err(e) => s.msg = Some(e.message()),
                }
            }
            other => state.lock().unwrap().msg = other.message(),
        }
        ctx.request_repaint();
    });
}

/// Delete the just-shared report (from the share-result area); on success reset `state` to
/// `Idle`, otherwise show the error.
pub fn spawn_delete_share(
    store_path: PathBuf,
    char_id: i64,
    id: String,
    state: SharedShare,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let outcome = match valid_session(&store_path, char_id) {
            None => DeleteOutcome::NeedLogin,
            Some(bearer) => delete(&api_base(), &bearer, &id),
        };
        *state.lock().unwrap() = match outcome {
            DeleteOutcome::Deleted => ShareStatus::Idle,
            other => ShareStatus::Error(other.message().unwrap_or_default()),
        };
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use br_core::battle::Battle;
    use rusqlite::{params, Connection};
    use std::io::Read;

    fn sample_doc() -> BattleReportDoc {
        let battle = Battle {
            engagements: Vec::new(),
            start: 1700,
            end: 1800,
            systems: vec![(30000142, "Jita".into(), 0.9)],
            sides: Vec::new(),
            kills: 0,
            isk: 0.0,
            ambiguous: false,
            suggested_splits: Vec::new(),
        };
        BattleReportDoc::new(battle, Vec::new(), Default::default(), Some("Test".into()), 1700)
    }

    #[test]
    fn gzip_round_trips_doc_json() {
        let doc = sample_doc();
        let original = serde_json::to_vec(&doc).unwrap();
        let gz = gzip_json(&doc).unwrap();
        // gzip magic header, and it isn't just the plaintext.
        assert_eq!(&gz[..2], &[0x1f, 0x8b]);
        let mut dec = flate2::read::GzDecoder::new(&gz[..]);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).unwrap();
        assert_eq!(out, original, "ungzip must reproduce the exact JSON");
        // And it parses back to an equal document.
        let back: BattleReportDoc = serde_json::from_slice(&out).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn mine_page_deserializes_from_server_shape() {
        // Matches crates/server ReportPage<ReportRow> for /api/br/mine (owner view).
        let json = r#"{
            "page": 1, "per_page": 2,
            "reports": [
                {"id":"abc123","title":"Big Fight","systems":["Jita","Perimeter"],
                 "started_at":"2026-06-22T18:30:45Z","ended_at":null,
                 "kills":42,"total_isk":1234567890.0,"side_names":["A","B"],
                 "uploader_name":"Pilot","views":17,"unlisted":false},
                {"id":"def456","title":null,"systems":[],
                 "started_at":null,"kills":0,"total_isk":0.0,
                 "side_names":[],"uploader_name":"Pilot","views":0,"unlisted":true}
            ]
        }"#;
        let page: ReportPage = serde_json::from_str(json).unwrap();
        assert_eq!(page.reports.len(), 2);
        let r0 = &page.reports[0];
        assert_eq!(r0.id, "abc123");
        assert_eq!(r0.title.as_deref(), Some("Big Fight"));
        assert_eq!(r0.systems, vec!["Jita", "Perimeter"]);
        assert_eq!(r0.kills, 42);
        assert_eq!(r0.views, 17);
        assert_eq!(r0.unlisted, Some(false));
        // No url field present → derived from the id.
        assert_eq!(r0.url("https://eve-spai.com"), "https://eve-spai.com/br/abc123");
        assert_eq!(page.reports[1].unlisted, Some(true));
    }

    #[test]
    fn create_response_deserializes() {
        let r: CreateResponse =
            serde_json::from_str(r#"{"id":"xyz","url":"https://eve-spai.com/br/xyz"}"#).unwrap();
        assert_eq!(r.id, "xyz");
        assert_eq!(r.url, "https://eve-spai.com/br/xyz");
    }

    #[test]
    fn bearer_decision_reuse_vs_refresh() {
        let now = 1_000_000;
        // Fresh token (well beyond skew) → reuse, refresh token untouched.
        assert_eq!(
            decide_bearer(Some("acc".into()), Some("ref".into()), now + 3600, now),
            BearerDecision::Reuse("acc".into())
        );
        // Expired token but a refresh token present → refresh.
        assert_eq!(
            decide_bearer(Some("acc".into()), Some("ref".into()), now - 1, now),
            BearerDecision::Refresh("ref".into())
        );
        // Within the skew window (≤60s to expiry) → refresh, not reuse.
        assert_eq!(
            decide_bearer(Some("acc".into()), Some("ref".into()), now + 30, now),
            BearerDecision::Refresh("ref".into())
        );
        // Expired and no refresh token → no usable credentials.
        assert_eq!(decide_bearer(Some("acc".into()), None, now - 1, now), BearerDecision::None);
        // No tokens at all.
        assert_eq!(decide_bearer(None, None, 0, now), BearerDecision::None);
        // Empty access string is treated as absent.
        assert_eq!(
            decide_bearer(Some(String::new()), Some("ref".into()), now + 3600, now),
            BearerDecision::Refresh("ref".into())
        );
    }

    #[test]
    fn session_deserializes_from_server_shape() {
        // Matches the locked POST /api/session contract.
        let json = r#"{
            "token": "sess.jwt.value",
            "expires_at": 1750000000,
            "character_id": 95123456,
            "character_name": "Pilot Name"
        }"#;
        let s: Session = serde_json::from_str(json).unwrap();
        assert_eq!(s.token, "sess.jwt.value");
        assert_eq!(s.expires_at, 1_750_000_000);
        assert_eq!(s.character_id, 95_123_456);
        assert_eq!(s.character_name, "Pilot Name");
    }

    #[test]
    fn session_action_reuse_vs_mint() {
        let now = 1_000_000;
        let fresh = Session {
            token: "sess".into(),
            expires_at: now + 3600,
            character_id: 1,
            character_name: "P".into(),
        };
        // Comfortably-valid cached session → reuse its token, no mint.
        assert_eq!(session_action(Some(&fresh), now), SessionAction::Reuse("sess".into()));
        // Empty cache → mint.
        assert_eq!(session_action(None, now), SessionAction::Mint);
        // Already expired → mint.
        let expired = Session { expires_at: now - 1, ..fresh.clone() };
        assert_eq!(session_action(Some(&expired), now), SessionAction::Mint);
        // Within the skew window (≤60s to expiry) → mint, not reuse.
        let near = Session { expires_at: now + 30, ..fresh.clone() };
        assert_eq!(session_action(Some(&near), now), SessionAction::Mint);
        // Exactly at the skew boundary still mints (strict `>`).
        let boundary = Session { expires_at: now + SESSION_SKEW_SECS, ..fresh };
        assert_eq!(session_action(Some(&boundary), now), SessionAction::Mint);
    }

    #[test]
    fn valid_bearer_reuses_fresh_cached_token() {
        // Scratch DB in a unique temp dir — never the user's real config (no-config-mutation).
        let dir = std::env::temp_dir().join(format!(
            "eve-spai-brshare-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scratch.db");
        // A fake character id with no keychain entry, so load_refresh returns None (read-only).
        let char_id: i64 = 999_999_999;
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE characters (id INTEGER PRIMARY KEY, name TEXT, expires_at INTEGER, scopes TEXT);",
            )
            .unwrap();
            let future = chrono::Utc::now().timestamp() + 3600;
            conn.execute(
                "INSERT INTO characters (id, name, expires_at, scopes) VALUES (?1, 'Scratch', ?2, '')",
                params![char_id, future],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO kv (key, value) VALUES (?1, 'cached-token')",
                params![format!("access:{char_id}")],
            )
            .unwrap();
        }
        // Fresh expiry → returns the cached token without any network/refresh.
        assert_eq!(valid_bearer(&path, char_id).as_deref(), Some("cached-token"));
        // Unknown character → None (prompt login).
        assert_eq!(valid_bearer(&path, 111_222_333), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
