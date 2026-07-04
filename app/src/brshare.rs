use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use br_core::battle::BattleReportDoc;
use crate::auth::{self, DEFAULT_CLIENT_ID};

pub const BR_API_BASE: &str = "https://eve-spai.com";

pub fn api_base() -> String {
    let raw = std::env::var("EVE_SPAI_API_BASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| BR_API_BASE.to_string());
    raw.trim_end_matches('/').to_string()
}

const REFRESH_SKEW_SECS: i64 = 60;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateResponse {
    pub id: String,
    pub url: String,
}

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
    #[serde(default)]
    pub url: Option<String>,
}

impl ReportRow {
    pub fn url(&self, base: &str) -> String {
        match self.url.as_deref().filter(|u| !u.is_empty()) {
            Some(u) => u.to_string(),
            None => format!("{base}/br/{}", self.id),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ReportPage {
    #[serde(default)]
    reports: Vec<ReportRow>,
}

#[derive(Debug, PartialEq)]
enum BearerDecision {
    Reuse(String),
    Refresh(String),
    None,
}

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

const SESSION_SKEW_SECS: i64 = 60;

#[derive(Debug, Clone, Deserialize)]
pub struct Session {
    pub token: String,
    pub expires_at: i64,
    pub character_id: i64,
    pub character_name: String,
}

fn session_cache() -> &'static Mutex<std::collections::HashMap<i64, Session>> {
    static CACHE: std::sync::OnceLock<Mutex<std::collections::HashMap<i64, Session>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

#[derive(Debug, PartialEq)]
enum SessionAction {
    Reuse(String),
    Mint,
}

fn session_action(cached: Option<&Session>, now: i64) -> SessionAction {
    match cached {
        Some(s) if s.expires_at > now + SESSION_SKEW_SECS => SessionAction::Reuse(s.token.clone()),
        _ => SessionAction::Mint,
    }
}

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

pub fn valid_session(store_path: &Path, char_id: i64) -> Option<String> {
    let now = chrono::Utc::now().timestamp();
    if let SessionAction::Reuse(token) =
        session_action(session_cache().lock().unwrap().get(&char_id), now)
    {
        return Some(token);
    }
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

fn http_client() -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION"), " (EVE intel tool)"))
        .timeout(Duration::from_secs(45))
        .build()
}

pub fn gzip_json(doc: &BattleReportDoc) -> std::io::Result<Vec<u8>> {
    let json = serde_json::to_vec(doc)?;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&json)?;
    enc.finish()
}

#[derive(Debug)]
pub enum ShareError {
    NeedLogin,
    TooLarge,
    Quota,
    Rejected,
    Network(String),
}

impl ShareError {
    pub fn message(&self) -> String {
        match self {
            ShareError::NeedLogin => {
                "Login expired. Re-authenticate on the Characters page, then try again.".into()
            }
            ShareError::TooLarge => "Report too large to share (over the server's size limit).".into(),
            ShareError::Quota => "Sharing quota reached. Try again later.".into(),
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

#[derive(Debug)]
pub enum DeleteOutcome {
    Deleted,
    NotOwner,
    NeedLogin,
    Error(String),
}

impl DeleteOutcome {
    pub fn message(&self) -> Option<String> {
        match self {
            DeleteOutcome::Deleted => None,
            DeleteOutcome::NotOwner => Some("You are not the owner of that report.".into()),
            DeleteOutcome::NeedLogin => {
                Some("Login expired. Re-authenticate on the Characters page.".into())
            }
            DeleteOutcome::Error(e) => Some(format!("Could not delete: {e}")),
        }
    }
}

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

#[derive(Default)]
pub struct MineState {
    pub status: MineStatus,
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
        BattleReportDoc::new(
            battle,
            Vec::new(),
            Default::default(),
            Some("Test".into()),
            1700,
            Default::default(),
            Default::default(),
        )
    }

    #[test]
    fn gzip_round_trips_doc_json() {
        let doc = sample_doc();
        let original = serde_json::to_vec(&doc).unwrap();
        let gz = gzip_json(&doc).unwrap();
        assert_eq!(&gz[..2], &[0x1f, 0x8b]);
        let mut dec = flate2::read::GzDecoder::new(&gz[..]);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).unwrap();
        assert_eq!(out, original, "ungzip must reproduce the exact JSON");
        let back: BattleReportDoc = serde_json::from_slice(&out).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn mine_page_deserializes_from_server_shape() {
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
        assert_eq!(
            decide_bearer(Some("acc".into()), Some("ref".into()), now + 3600, now),
            BearerDecision::Reuse("acc".into())
        );
        assert_eq!(
            decide_bearer(Some("acc".into()), Some("ref".into()), now - 1, now),
            BearerDecision::Refresh("ref".into())
        );
        assert_eq!(
            decide_bearer(Some("acc".into()), Some("ref".into()), now + 30, now),
            BearerDecision::Refresh("ref".into())
        );
        assert_eq!(decide_bearer(Some("acc".into()), None, now - 1, now), BearerDecision::None);
        assert_eq!(decide_bearer(None, None, 0, now), BearerDecision::None);
        assert_eq!(
            decide_bearer(Some(String::new()), Some("ref".into()), now + 3600, now),
            BearerDecision::Refresh("ref".into())
        );
    }

    #[test]
    fn session_deserializes_from_server_shape() {
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
        assert_eq!(session_action(Some(&fresh), now), SessionAction::Reuse("sess".into()));
        assert_eq!(session_action(None, now), SessionAction::Mint);
        let expired = Session { expires_at: now - 1, ..fresh.clone() };
        assert_eq!(session_action(Some(&expired), now), SessionAction::Mint);
        let near = Session { expires_at: now + 30, ..fresh.clone() };
        assert_eq!(session_action(Some(&near), now), SessionAction::Mint);
        let boundary = Session { expires_at: now + SESSION_SKEW_SECS, ..fresh };
        assert_eq!(session_action(Some(&boundary), now), SessionAction::Mint);
    }

    #[test]
    fn valid_bearer_reuses_fresh_cached_token() {
        let dir = std::env::temp_dir().join(format!(
            "eve-spai-brshare-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scratch.db");
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
        assert_eq!(valid_bearer(&path, char_id).as_deref(), Some("cached-token"));
        assert_eq!(valid_bearer(&path, 111_222_333), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
